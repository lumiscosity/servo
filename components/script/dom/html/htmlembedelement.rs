/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use std::cell::Cell;
use std::sync::{Arc, Mutex};

use base::id::{BrowsingContextId, PipelineId, WebViewId};
use constellation_traits::{IFrameLoadInfo, IFrameLoadInfoWithData, JsEvalResult, LoadData, LoadOrigin, NavigationHistoryBehavior, ScriptToConstellationMessage};
use content_security_policy::Destination;
use dom_struct::dom_struct;
use embedder_traits::ViewportDetails;
use html5ever::{LocalName, Prefix};
use js::rust::HandleObject;
use net_traits::request::{CredentialsMode, Referrer, RequestBuilder, RequestMode};
use net_traits::{FetchResponseListener, ResourceFetchTiming, ResourceTimingType};
use script_bindings::inheritance::Castable;
use script_bindings::script_runtime::CanGc;
use script_bindings::str::{DOMString, USVString};
use script_traits::{NewLayoutInfo, UpdatePipelineIdReason};
use servo_url::ServoUrl;
use style::attr::{AttrValue, LengthOrPercentageOrAuto};
use xml5ever::{local_name, ns};

use crate::document_loader::{LoadBlocker, LoadType};
use crate::dom::bindings::cell::DomRefCell;
use crate::dom::bindings::codegen::Bindings::HTMLEmbedElementBinding::HTMLEmbedElementMethods;
use crate::dom::bindings::refcounted::Trusted;
use crate::dom::bindings::reflector::DomGlobal;
use crate::dom::bindings::root::{DomRoot, LayoutDom};
use crate::dom::document::Document;
use crate::dom::element::LayoutElementHelpers;
use crate::dom::htmlelement::HTMLElement;
use crate::dom::htmliframeelement::PipelineType;
use crate::dom::node::{BindContext, Node, NodeDamage, NodeTraits, UnbindContext};
use crate::dom::performanceresourcetiming::InitiatorType;
use crate::dom::types::{Element, EventTarget, HTMLMediaElement};
use crate::dom::virtualmethods::VirtualMethods;
use crate::network_listener::{PreInvoke, ResourceTimingListener, submit_timing};
use crate::ScriptThread;

#[dom_struct]
pub(crate) struct HTMLEmbedElement {
    htmlelement: HTMLElement,
    #[no_trace]
    webview_id: Cell<Option<WebViewId>>,
    #[no_trace]
    browsing_context_id: Cell<Option<BrowsingContextId>>,
    #[no_trace]
    pipeline_id: Cell<Option<PipelineId>>,
    #[no_trace]
    pending_pipeline_id: Cell<Option<PipelineId>>,
    #[no_trace]
    about_blank_pipeline_id: Cell<Option<PipelineId>>,
    load_blocker: DomRefCell<Option<LoadBlocker>>,
}

impl HTMLEmbedElement {
    fn new_inherited(
        local_name: LocalName,
        prefix: Option<Prefix>,
        document: &Document,
    ) -> HTMLEmbedElement {
        HTMLEmbedElement {
            htmlelement: HTMLElement::new_inherited(local_name, prefix, document),
            browsing_context_id: Cell::new(None),
            webview_id: Cell::new(None),
            pipeline_id: Cell::new(None),
            pending_pipeline_id: Cell::new(None),
            about_blank_pipeline_id: Cell::new(None),
            load_blocker: DomRefCell::new(None),
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

    fn create_child_navigable(&self, can_gc: CanGc) {
        println!("creating child navigable");
        let url = ServoUrl::parse("about:blank").unwrap();
        let document = self.owner_document();
        let window = self.owner_window();
        let pipeline_id = Some(window.pipeline_id());
        let mut load_data = LoadData::new(
            LoadOrigin::Script(document.origin().immutable().clone()),
                                          url,
                                          pipeline_id,
                                          window.as_global_scope().get_referrer(),
                                          document.get_referrer_policy(),
                                          Some(window.as_global_scope().is_secure_context()),
                                          Some(document.insecure_requests_policy()),
                                          document.has_trustworthy_ancestor_or_current_origin(),
        );
        load_data.destination = Destination::Embed;
        load_data.policy_container = Some(window.as_global_scope().policy_container());
        let browsing_context_id = BrowsingContextId::new();
        let webview_id = window.window_proxy().webview_id();
        self.pipeline_id.set(None);
        self.pending_pipeline_id.set(None);
        self.webview_id.set(Some(webview_id));
        self.browsing_context_id.set(Some(browsing_context_id));
        self.start_new_pipeline(
            load_data,
            PipelineType::InitialAboutBlank,
            NavigationHistoryBehavior::Push,
            can_gc,
        );
    }

    fn destroy_child_navigable(&self) {
        self.pipeline_id.set(None);
        self.pending_pipeline_id.set(None);
        self.about_blank_pipeline_id.set(None);
        self.webview_id.set(None);
        self.browsing_context_id.set(None);
    }

    fn start_new_pipeline(
        &self,
        mut load_data: LoadData,
        pipeline_type: PipelineType,
        history_handling: NavigationHistoryBehavior,
        can_gc: CanGc,
    ) {
        let sandboxed = constellation_traits::IFrameSandboxState::IFrameUnsandboxed;

        let browsing_context_id = match self.browsing_context_id() {
            None => return warn!("Attempted to start a new pipeline on an unattached iframe."),
            Some(id) => id,
        };

        let webview_id = match self.webview_id() {
            None => return warn!("Attempted to start a new pipeline on an unattached iframe."),
            Some(id) => id,
        };

        let document = self.owner_document();

        {
            let load_blocker = &self.load_blocker;
            // Any oustanding load is finished from the point of view of the blocked
            // document; the new navigation will continue blocking it.
            LoadBlocker::terminate(load_blocker, can_gc);
        }

        if load_data.url.scheme() == "javascript" {
            let window_proxy = self.browsing_context_id
                .get()
                .and_then(ScriptThread::find_window_proxy);

            if let Some(window_proxy) = window_proxy {
                if !ScriptThread::navigate_to_javascript_url(
                    &document.global(),
                                                             &window_proxy.global(),
                                                             &mut load_data,
                                                             Some(self.upcast()),
                                                             can_gc,
                ) {
                    return;
                }
            }
        }

        match load_data.js_eval_result {
            Some(JsEvalResult::NoContent) => (),
            _ => {
                let mut load_blocker = self.load_blocker.borrow_mut();
                *load_blocker = Some(LoadBlocker::new(
                    &document,
                    LoadType::Subframe(load_data.url.clone()),
                ));
            },
        };

        let window = self.owner_window();
        let old_pipeline_id = self.pipeline_id();
        let new_pipeline_id = PipelineId::new();
        self.pending_pipeline_id.set(Some(new_pipeline_id));

        let load_info = IFrameLoadInfo {
            parent_pipeline_id: window.pipeline_id(),
            browsing_context_id,
            webview_id,
            new_pipeline_id,
            is_private: false, // FIXME
            inherited_secure_context: load_data.inherited_secure_context,
            history_handling,
        };

        let viewport_details = window
        .get_iframe_viewport_details_if_known(browsing_context_id)
        .unwrap_or_else(|| ViewportDetails {
            hidpi_scale_factor: window.device_pixel_ratio(),
                        ..Default::default()
        });

        match pipeline_type {
            PipelineType::InitialAboutBlank => {
                self.about_blank_pipeline_id.set(Some(new_pipeline_id));

                let load_info = IFrameLoadInfoWithData {
                    info: load_info,
                    load_data: load_data.clone(),
                    old_pipeline_id,
                    sandbox: sandboxed,
                    viewport_details,
                    theme: window.theme(),
                };
                window
                .as_global_scope()
                .script_to_constellation_chan()
                .send(ScriptToConstellationMessage::ScriptNewIFrame(load_info))
                .unwrap();

                let new_layout_info = NewLayoutInfo {
                    parent_info: Some(window.pipeline_id()),
                    new_pipeline_id,
                    browsing_context_id,
                    webview_id,
                    opener: None,
                    load_data,
                    viewport_details,
                    theme: window.theme(),
                };

                self.pipeline_id.set(Some(new_pipeline_id));
                ScriptThread::process_attach_layout(new_layout_info, document.origin().clone());
            },
            PipelineType::Navigation => {
                let load_info = IFrameLoadInfoWithData {
                    info: load_info,
                    load_data,
                    old_pipeline_id,
                    sandbox: sandboxed,
                    viewport_details,
                    theme: window.theme(),
                };
                window
                .as_global_scope()
                .script_to_constellation_chan()
                .send(ScriptToConstellationMessage::ScriptLoadedURLInIFrame(
                    load_info,
                ))
                .unwrap();
            },
        }
    }

    pub(crate) fn update_pipeline_id(
        &self,
        new_pipeline_id: PipelineId,
        reason: UpdatePipelineIdReason,
        can_gc: CanGc,
    ) {
        if self.pending_pipeline_id.get() != Some(new_pipeline_id) &&
            reason == UpdatePipelineIdReason::Navigation
            {
                return;
            }

            self.pipeline_id.set(Some(new_pipeline_id));

        // Only terminate the load blocker if the pipeline id was updated due to a traversal.
        // The load blocker will be terminated for a navigation in iframe_load_event_steps.
        if reason == UpdatePipelineIdReason::Traversal {
            let blocker = &self.load_blocker;
            LoadBlocker::terminate(blocker, can_gc);
        }

        self.upcast::<Node>().dirty(NodeDamage::Other);
    }

    pub(crate) fn navigate_or_reload_content_navigable(
        &self,
        load_data: LoadData,
        history_handling: NavigationHistoryBehavior,
        can_gc: CanGc,
    ) {
        self.start_new_pipeline(
            load_data,
            PipelineType::Navigation,
            history_handling,
            can_gc,
        );
    }

    #[inline]
    pub(crate) fn pipeline_id(&self) -> Option<PipelineId> {
        self.pipeline_id.get()
    }

    #[inline]
    pub(crate) fn browsing_context_id(&self) -> Option<BrowsingContextId> {
        self.browsing_context_id.get()
    }

    #[inline]
    pub(crate) fn webview_id(&self) -> Option<WebViewId> {
        self.webview_id.get()
    }

    /// <https://html.spec.whatwg.org/multipage/#concept-embed-active>
    fn is_potentially_active(&self) -> bool {
        let element = self.upcast::<Element>();
        let node = self.upcast::<Node>();
        let src_attr = &local_name!("src");
        let type_attr = &local_name!("type");
        // The element is in a document or was in a document the last time the event loop reached step 1.
        let in_a_document = node.is_in_a_document_tree();
        // The element's node document is fully active.
        let node_document_active = node.owner_doc().is_fully_active();
        // The element has either a src attribute set or a type attribute set (or both).
        let has_src_or_type = element.has_attribute(src_attr) || element.has_attribute(type_attr);
        // The element's src attribute is either absent or its value is not the empty string.
        let src_attr_absent_or_not_empty: bool;
        if element.has_attribute(src_attr) {
            src_attr_absent_or_not_empty = !element.get_string_attribute(src_attr).is_empty();
        } else {
            src_attr_absent_or_not_empty = true;
        }
        // The element is not a descendant of a media element.
        let not_media_element_descendant = node
            .ancestors()
            .find(|ancestor| ancestor.downcast::<HTMLMediaElement>().is_some())
            .is_none();
        // TODO: The element is not a descendant of an object element that is not showing its fallback content.
        //       Blocked by the object element not having a concept of fallback content yet.
        // The element is being rendered, or was being rendered the last time the event loop reached step 1.
        // See https://html.spec.whatwg.org/multipage/#being-rendered.
        let is_rendered = element.has_css_layout_box();

        if !in_a_document {
            println!("in_a_document failed");
        }

        if !node_document_active {
            println!("node_document_active failed");
        }

        if !has_src_or_type {
            println!("has_src_or_type failed");
        }

        if !src_attr_absent_or_not_empty {
            println!("src_attr_absent_or_not_empty failed");
        }

        if !not_media_element_descendant {
            println!("not_media_element_descendant failed");
        }

        if !is_rendered {
            println!("is_rendered failed");
        }

        in_a_document &&
            node_document_active &&
            has_src_or_type &&
            src_attr_absent_or_not_empty &&
            not_media_element_descendant &&
            is_rendered
    }

    /// <https://html.spec.whatwg.org/multipage/#the-embed-element-setup-steps>
    /// TODO: We need to hook into a lot of places to fire this from. Refer to the linked paragraph
    ///       and the `potentially_active` function above.
    pub(crate) fn setup(&self) {
        println!("running setup steps");
        let this = Trusted::new(self);
        self.owner_document().global().task_manager().embed_task_source().queue(task!(setup_ok: move || {
            let binding = this.root();
            let element = binding.upcast::<Element>();
            let document = element.owner_document();
            let global = document.global();
            let src_attr = &local_name!("src");
            // TODO: 1. If another task has since been queued to run the embed element setup steps for element, then return.
            // 2. If element has a src attribute set, then:
            if element.has_attribute(src_attr) {
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

                // 4. Fetch request, with processResponse set to the following steps
                // given response response:
                // (Continued in EmbedSetupFetchListener's impls)
                global.fetch(
                    request,
                    Arc::new(Mutex::new(EmbedSetupFetchListener {
                            element: this,
                            url: url.clone(),
                            resource_timing: ResourceFetchTiming::new(ResourceTimingType::Navigation),
                    })),
                    global.task_manager().embed_task_source().into()
                )
            } else {
                // 3. Otherwise, display no plugin for element.
                binding.display_no_plugin();
            }
        }))
    }

    fn determine_content_type(&self, response: &Result<net_traits::FetchMetadata, net_traits::NetworkError>) -> Option<DOMString> {
        if let Ok(_metadata) = response {
            let element = self.upcast::<Element>();
            // 1. If element has a type attribute, and that attribute's value is a type
            // that a plugin supports, then return the value of the type attribute.
            let type_attr = &local_name!("type");
            if element.has_attribute(type_attr) {
                // TODO: Implement plugins!
                println!("{:?}", element.get_string_attribute(type_attr));
                return Some(element.get_string_attribute(type_attr));
            }
            // TODO: 2. If the path component of response's url matches a pattern that
            // a plugin supports, then return the type that that plugin can handle.
            // TODO: 3. If response has explicit Content-Type metadata, and that value
            // is a type that a plugin supports, then return that value.
        }

        // 4. Return null.
        None
    }

    /// <https://html.spec.whatwg.org/multipage/#display-no-plugin>
    pub(crate) fn display_no_plugin(&self) {
        // 1. Destroy a child navigable given element.
        self.destroy_child_navigable();
        // TODO: 2. Display an indication that no plugin could be found for element,
        // as the contents of element.
        // 3. element now represents nothing.
    }
}

pub(crate) trait HTMLEmbedElementLayoutMethods {
    fn pipeline_id(self) -> Option<PipelineId>;
    fn browsing_context_id(self) -> Option<BrowsingContextId>;
    fn get_width(self) -> LengthOrPercentageOrAuto;
    fn get_height(self) -> LengthOrPercentageOrAuto;
}

impl HTMLEmbedElementLayoutMethods for LayoutDom<'_, HTMLEmbedElement> {
    #[inline]
    fn pipeline_id(self) -> Option<PipelineId> {
        (self.unsafe_get()).pipeline_id.get()
    }

    #[inline]
    fn browsing_context_id(self) -> Option<BrowsingContextId> {
        (self.unsafe_get()).browsing_context_id.get()
    }

    fn get_width(self) -> LengthOrPercentageOrAuto {
        self.upcast::<Element>()
        .get_attr_for_layout(&ns!(), &local_name!("width"))
        .map(AttrValue::as_dimension)
        .cloned()
        .unwrap_or(LengthOrPercentageOrAuto::Auto)
    }

    fn get_height(self) -> LengthOrPercentageOrAuto {
        self.upcast::<Element>()
        .get_attr_for_layout(&ns!(), &local_name!("height"))
        .map(AttrValue::as_dimension)
        .cloned()
        .unwrap_or(LengthOrPercentageOrAuto::Auto)
    }
}

impl HTMLEmbedElementMethods<crate::DomTypeHolder> for HTMLEmbedElement {
    // https://html.spec.whatwg.org/multipage/#dom-embed-src
    make_url_getter!(Src, "src");
    fn SetSrc(&self, value: USVString) {
        let element = self.upcast::<Element>();
        element.set_url_attribute(&html5ever::local_name!("src"), value, CanGc::note());
        if self.is_potentially_active() {
            self.setup();
        } else {
            // TODO: Unload plugin here.
        }
    }

    // https://html.spec.whatwg.org/multipage/#dom-embed-type
    make_getter!(Type, "type");
    fn SetType(&self, value: DOMString) {
        let element = self.upcast::<Element>();
        element.set_string_attribute(&html5ever::local_name!("type"), value, CanGc::note());
        if self.is_potentially_active() {
            self.setup();
        } else {
            // TODO: Unload plugin here.
        }
    }

    // https://html.spec.whatwg.org/multipage/#dom-embed-width
    make_getter!(Width, "width");
    make_dimension_setter!(SetWidth, "width");

    // https://html.spec.whatwg.org/multipage/#dom-embed-height
    make_getter!(Height, "height");
    make_dimension_setter!(SetHeight, "height");

    // https://html.spec.whatwg.org/multipage/#dom-media-getsvgdocument
    fn GetSVGDocument(&self) -> Option<DomRoot<Document>> {
        // TODO: 1. Let document be this's content document.
        // TODO: 2. If document is non-null and was created by the page load processing
        // model for XML files section because the computed type of the resource in the
        // navigate algorithm was image/svg+xml, then return document.

        // 3. Return null.
        None
    }

    // https://html.spec.whatwg.org/multipage/#dom-embed-align
    make_getter!(Align, "align");
    make_setter!(SetAlign, "align");

    // https://html.spec.whatwg.org/multipage/#dom-embed-name
    make_getter!(Name, "name");
    make_setter!(SetName, "name");
}

impl VirtualMethods for HTMLEmbedElement {
    fn super_type(&self) -> Option<&dyn VirtualMethods> {
        Some(self.upcast::<HTMLElement>() as &dyn VirtualMethods)
    }

    fn bind_to_tree(&self, context: &BindContext, can_gc: CanGc) {
        if let Some(s) = self.super_type() {
            s.bind_to_tree(context, can_gc);
        }
        println!("embed element bound to tree");
        if self.is_potentially_active() {
            self.setup();
        }
    }

    fn unbind_from_tree(&self, context: &UnbindContext, can_gc: CanGc) {
        if let Some(s) = self.super_type() {
            s.unbind_from_tree(context, can_gc);
        }
        // TODO: Unload plugin here.
    }
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
        let rooted = self.element.root();
        // TODO: 1. If another task has since been queued to run the embed element
        // setup steps for element, then return.
        // 2. If response is a network error, then fire an event named load at
        // element, and return.
        if metadata.is_err() {
            rooted.upcast::<EventTarget>()
                .fire_event(atom!("load"), CanGc::note());
            return;
        }
        // 3. Let type be the result of determining the type of content given
        // element and response.
        // 4. Switch on type:
        if let Some(_type_) = rooted.determine_content_type(&metadata) {
            let data = metadata.unwrap();
            // 1. If element's content navigable is null, then create
            // a new child navigable for element.
            if rooted.webview_id == None.into() {
                rooted.create_child_navigable(CanGc::note());
            }
            // 2. Navigate element's content navigable to response's URL using
            // element's node document, with response set to response, and
            // historyHandling set to "replace".
            rooted.navigate_or_reload_content_navigable(LoadData::new(
                LoadOrigin::Script(rooted.owner_document().origin().immutable().clone()),
                data.metadata().final_url.clone(),
                rooted.pipeline_id(), Referrer::NoReferrer, data.metadata().referrer_policy, None, None, true),
                                                        NavigationHistoryBehavior::Replace,
                                                        CanGc::note());
            // TODO: 3. element now represents its content navigable.
        } else {
            // 1. Display no plugin for element.
            rooted.display_no_plugin();
        }
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

    fn process_csp_violations(
        &mut self,
        _request_id: net_traits::request::RequestId,
        _violations: Vec<content_security_policy::Violation>,
    ) {
    }
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
        true // TODO: Is this correct?
        // There's an inverse situation in step 1 of the setup task
        // and step 1 of the fetch it makes, where instead of
        // cancelling the new request we return early out of the existing one.
    }
}
