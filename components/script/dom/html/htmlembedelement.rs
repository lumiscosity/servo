/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use std::sync::{Arc, Mutex};

use content_security_policy::Destination;
use dom_struct::dom_struct;
use html5ever::{LocalName, Prefix};
use js::rust::HandleObject;
use net_traits::request::{CredentialsMode, RequestMode};
use net_traits::request::{Referrer, RequestBuilder};
use net_traits::{FetchResponseListener, NetworkError, ResourceFetchTiming, ResourceTimingType};
use script_bindings::inheritance::Castable;
use script_bindings::str::{DOMString, USVString};
use servo_url::ServoUrl;
use style::attr::AttrValue;
use xml5ever::local_name;

use crate::dom::bindings::codegen::Bindings::HTMLEmbedElementBinding::HTMLEmbedElementMethods;
use crate::dom::bindings::refcounted::Trusted;
use crate::dom::bindings::reflector::DomGlobal;
use crate::dom::bindings::root::DomRoot;
use crate::dom::document::Document;
use crate::dom::htmlelement::HTMLElement;
use crate::dom::node::{Node, NodeTraits};
use crate::dom::performanceresourcetiming::InitiatorType;
use crate::dom::types::{Element, EventTarget, HTMLMediaElement};
use crate::network_listener::{submit_timing, PreInvoke, ResourceTimingListener};
use crate::script_runtime::CanGc;

#[dom_struct]
pub(crate) struct HTMLEmbedElement {
    htmlelement: HTMLElement,
}

impl HTMLEmbedElement {
    fn new_inherited(
        local_name: LocalName,
        prefix: Option<Prefix>,
        document: &Document,
    ) -> HTMLEmbedElement {
        HTMLEmbedElement {
            htmlelement: HTMLElement::new_inherited(local_name, prefix, document),
        }
    }

    #[cfg_attr(crown, allow(crown::unrooted_must_root))]
    pub(crate) fn new(
        local_name: LocalName,
        prefix: Option<Prefix>,
        document: &Document,
        proto: Option<HandleObject>,
        can_gc: CanGc,
    ) -> DomRoot<HTMLEmbedElement> {
        Node::reflect_node_with_proto(
            Box::new(HTMLEmbedElement::new_inherited(
                local_name, prefix, document,
            )),
            document,
            proto,
            can_gc,
        )
    }

    /// TODO: get link to this. There's no heading, but it's above concept-embed-active
    fn represents_nothing(&self) -> bool {
        let element = self.upcast::<Element>();
        let node = self.upcast::<Node>();
        let src_attr = &local_name!("src");
        let type_attr = &local_name!("type");
        // The element has neither a src attribute nor a type attribute.
        let neither_src_nor_type = !element.has_attribute(src_attr)
            && !element.has_attribute(type_attr);
        // The element has a media element ancestor.
        let media_element_descendant = node.ancestors().find(|ancestor| {
            ancestor.downcast::<HTMLMediaElement>().is_some()
        }).is_some();
        // TODO: The element has an ancestor object element that is not showing its fallback content.
        //       Blocked by the object element not having a concept of fallback content yet.
        neither_src_nor_type || media_element_descendant
    }

    /// https://html.spec.whatwg.org/multipage/iframe-embed-object.html#concept-embed-active
    fn potentially_active(&self) -> bool {
        let element = self.upcast::<Element>();
        let node = self.upcast::<Node>();
        let src_attr = &local_name!("src");
        let type_attr = &local_name!("type");
        // The element is in a document or was in a document the last time the event loop reached step 1.
        let in_a_document = node.is_in_a_document_tree();
        // The element's node document is fully active.
        let node_document_active = node.owner_doc().is_fully_active();
        // The element has either a src attribute set or a type attribute set (or both).
        let has_src_or_type = element.has_attribute(src_attr)
            || element.has_attribute(type_attr);
        // The element's src attribute is either absent or its value is not the empty string.
        let src_attr_absent_or_not_empty: bool;
        if element.has_attribute(src_attr) {
            src_attr_absent_or_not_empty = !element.get_string_attribute(src_attr).is_empty();
        } else {
            src_attr_absent_or_not_empty = true;
        }
        // The element is not a descendant of a media element.
        let not_media_element_descendant = node.ancestors().find(|ancestor| {
            ancestor.downcast::<HTMLMediaElement>().is_some()
        }).is_none();
        // TODO: The element is not a descendant of an object element that is not showing its fallback content.
        //       Blocked by the object element not having a concept of fallback content yet.
        // The element is being rendered, or was being rendered the last time the event loop reached step 1.
        // <https://html.spec.whatwg.org/multipage/rendering.html#being-rendered>
        let is_rendered = element.has_css_layout_box();
        in_a_document && node_document_active && has_src_or_type && src_attr_absent_or_not_empty
        && not_media_element_descendant && is_rendered
    }

    /// https://html.spec.whatwg.org/multipage/iframe-embed-object.html#the-embed-element-setup-steps
    /// TODO: We need to hook into a lot of places to fire this from. Refer to the linked paragraph
    ///       and the `potentially_active` function above.
    pub(crate) fn setup(&self) {
        let this = Trusted::new(self);
        self.owner_document().global().task_manager().embed_task_source().queue(task!(setup_ok: move || {
            let binding = this.root();
            let element = binding.upcast::<Element>();
            let document = element.owner_document();
            let global = document.global();
            let src_attr = &local_name!("src");
            // TODO: 1. If another task has since been queued to run the embed element setup steps for element, then return.
            // 2. If element has a src attribute set, then:
            if element.has_attribute(src_attr){
                // 1. Let url be the result of encoding-parsing a URL given element's src
                // attribute's value, relative to element's node document.
                // 2. If url is failure, then return.
                let url = match document.encoding_parse_a_url(
                    &element.get_string_attribute(src_attr).to_string()
                    ) {
                        Ok(value) => value,
                        Err(_) => return,
                };

                // 3. Let request be a new request whose URL is url, client is element's node
                // document's relevant settings object, destination is "embed", credentials
                // mode is "include", mode is "navigate", initiator type is "embed", and
                // whose use-URL-credentials flag is set.
                // TODO: Is the client field what we do with the webview ID?
                let request = RequestBuilder::new(
                    Some(document.webview_id()),
                    url.clone(),
                    Referrer::NoReferrer
                )
                    .destination(Destination::Embed)
                    .credentials_mode(CredentialsMode::Include)
                    .mode(RequestMode::Navigate)
                    .use_url_credentials(true);

                // 4. Fetch request, with processResponse set to the following steps given
                // response response:
                // (Continued in EmbedSetupFetchListener's impls)
                global.fetch(
                    request,
                    Arc::new(Mutex::new(EmbedSetupFetchListener {
                            element: this,
                            url: url.clone(),
                            resource_timing: ResourceFetchTiming::new(ResourceTimingType::Resource),
                    })),
                    global.task_manager().embed_task_source().into()
                )
            }
        }))
    }
}

impl HTMLEmbedElementMethods<crate::DomTypeHolder> for HTMLEmbedElement {
    // https://html.spec.whatwg.org/multipage/#dom-embed-src
    make_url_getter!(Src, "src");
    // https://html.spec.whatwg.org/multipage/#dom-embed-src
    make_url_setter!(SetSrc, "src");

    // https://html.spec.whatwg.org/multipage/#dom-embed-type
    make_getter!(Type, "type");
    // https://html.spec.whatwg.org/multipage/#dom-embed-type
    make_setter!(SetType, "type");

    // https://html.spec.whatwg.org/multipage/#dom-embed-width
    make_getter!(Width, "width");
    // https://html.spec.whatwg.org/multipage/#dom-embed-width
    make_dimension_setter!(SetWidth, "width");

    // https://html.spec.whatwg.org/multipage/#dom-embed-height
    make_getter!(Height, "height");
    // https://html.spec.whatwg.org/multipage/#dom-embed-height
    make_dimension_setter!(SetHeight, "height");

    // https://html.spec.whatwg.org/multipage/embedded-content-other.html#dom-media-getsvgdocument
    // TODO: According to the spec, <iframe> and <object> should also have this!
    // Maybe it should be generic between the three somehow? SVGDocument trait with a getter/setter? Macro?
    fn GetSVGDocument(&self, ) -> Option<DomRoot<<crate::DomTypeHolder as script_bindings::DomTypes>::Document>> {
        todo!()
    }

    // https://html.spec.whatwg.org/multipage/obsolete.html#dom-embed-align
    make_getter!(Align, "align");
    // https://html.spec.whatwg.org/multipage/obsolete.html#dom-embed-align
    make_setter!(SetAlign, "align");

    // https://html.spec.whatwg.org/multipage/obsolete.html#dom-embed-name
    make_getter!(Name, "name");
    // https://html.spec.whatwg.org/multipage/obsolete.html#dom-embed-name
    make_setter!(SetName, "name");
}

struct EmbedSetupFetchListener {
    /// The <embed> element responsible for this fetch request.
    element: Trusted<HTMLEmbedElement>,
    /// URL of this request.
    url: ServoUrl,
    /// Timing data for this resource.
    resource_timing: ResourceFetchTiming,
}

impl FetchResponseListener for EmbedSetupFetchListener {
    fn process_request_body(&mut self, _: net_traits::request::RequestId) {}

    fn process_request_eof(&mut self, _: net_traits::request::RequestId) {}

    fn process_response(
        &mut self,
        _: net_traits::request::RequestId,
        metadata: Result<net_traits::FetchMetadata, net_traits::NetworkError>,
    ) {
        // TODO: 1. If another task has since been queued to run the embed element
        // setup steps for element, then return.
        // 2. If response is a network error, then fire an event named load at
        // element, and return.
        if metadata.is_err() {
            self.element.root().upcast::<EventTarget>()
                .fire_event(atom!("load"), CanGc::note());
            return;
        }
        // TODO: 3. Let type be the result of determining the type of content given
        // element and response.
        // 4. Switch on type:
        // Blocked on plugins, mostly.
    }

    fn process_response_chunk(&mut self, _: net_traits::request::RequestId, chunk: Vec<u8>) {
        _ = chunk
    }

    fn process_response_eof(
        &mut self,
        _: net_traits::request::RequestId,
        response: Result<net_traits::ResourceFetchTiming, net_traits::NetworkError>,
    ) {
        _ = response

    }

    fn resource_timing_mut(&mut self) -> &mut ResourceFetchTiming {
        &mut self.resource_timing
    }

    fn resource_timing(&self) -> &ResourceFetchTiming {
        &self.resource_timing
    }

    fn submit_resource_timing(&mut self) {
        submit_timing(self, CanGc::note())
    }

    fn process_csp_violations(&mut self, _request_id: net_traits::request::RequestId, _violations: Vec<content_security_policy::Violation>) {}
}

impl ResourceTimingListener for EmbedSetupFetchListener {
    fn resource_timing_information(&self) -> (InitiatorType, ServoUrl) {
        (
            InitiatorType::LocalName("embed".to_string()),
            self.url.clone(),
        )
    }

    fn resource_timing_global(&self) -> DomRoot<crate::dom::types::GlobalScope> {
        self.element.root().global()
    }
}

impl PreInvoke for EmbedSetupFetchListener {
    fn should_invoke(&self) -> bool {
        true // TODO: Is this where we check if the task has been fired for step 1 of the setup steps and the inner task?
    }
}
