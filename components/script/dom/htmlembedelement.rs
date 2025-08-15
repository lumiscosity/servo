/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use content_security_policy::Destination;
use dom_struct::dom_struct;
use html5ever::{LocalName, Prefix};
use js::rust::HandleObject;
use net_traits::request::{CredentialsMode, RequestMode};
use net_traits::request::{Referrer, RequestBuilder};
use script_bindings::inheritance::Castable;
use script_bindings::str::DOMString;
use url::Url;
use xml5ever::local_name;
use servo_url::ServoUrl;

use crate::dom::bindings::codegen::Bindings::HTMLEmbedElementBinding::HTMLEmbedElementMethods;
use crate::dom::bindings::root::DomRoot;
use crate::dom::document::Document;
use crate::dom::htmlelement::HTMLElement;
use crate::dom::node::Node;
use crate::dom::types::Element;
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
        let src_attr = &local_name!("src");
        let type_attr = &local_name!("type");
        // The element has neither a src attribute nor a type attribute.
        let neither_src_nor_type = !element.has_attribute(src_attr)
            && !element.has_attribute(type_attr);
        // TODO: The element has a media element ancestor.
        // TODO: The element has an ancestor object element that is not showing its fallback content.
        neither_src_nor_type
    }

    /// https://html.spec.whatwg.org/multipage/iframe-embed-object.html#concept-embed-active
    fn potentially_active(&self) -> bool {
        let element = self.upcast::<Element>();
        let src_attr = &local_name!("src");
        let type_attr = &local_name!("type");
        // TODO: The element is in a document or was in a document the last time the event loop reached step 1.
        // TODO: The element's node document is fully active.
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
        // TODO: The element is not a descendant of a media element.
        // TODO: The element is not a descendant of an object element that is not showing its fallback content.
        // TODO: The element is being rendered, or was being rendered the last time the event loop reached step 1.
        has_src_or_type && src_attr_absent_or_not_empty
    }

    /// https://html.spec.whatwg.org/multipage/iframe-embed-object.html#the-embed-element-setup-steps
    /// TODO: the line above says this is called by the user agent, so I assume
    /// it must be available from somewhere else, hence the pub(crate)?
    /// Need to investigate this - there's a convenient link, at least!
    pub(crate) fn element_setup(&self) {
        let element = self.upcast::<Element>();
        let src_attr = &local_name!("src");
        let type_attr = &local_name!("type");
        // TODO: 1. If another task has since been queued to run the embed element setup steps for element, then return.
        // 2. If element has a src attribute set, then:
        if element.has_attribute(src_attr){
            // 1. Let url be the result of encoding-parsing a URL given element's src
            // attribute's value, relative to element's node document.
            // 2. If url is failure, then return.
            let url_options = Url::options();
            // TODO: Do the element encoding. How do I get the node document?
            let url = match url_options.parse(
                &element.get_string_attribute(src_attr).to_string()
                ) {
                    Ok(value) => ServoUrl::from_url(value),
                    Err(_) => return,
            };
            // 3. Let request be a new request whose URL is url, client is element's node
            // document's relevant settings object, destination is "embed", credentials
            // mode is "include", mode is "navigate", initiator type is "embed", and
            // whose use-URL-credentials flag is set.
            // TODO: What goes in webview in the builder? What is the element's node document's relevant
            // settings object and how do I get it?
            let request = RequestBuilder::new(None, url, Referrer::NoReferrer)
                .destination(Destination::Embed)
                .credentials_mode(CredentialsMode::Include)
                .mode(RequestMode::Navigate)
                // TODO: How to set initiator type?
                //.initiator_type(InitiatorType::Embed)
                .use_url_credentials(true)
                .build();
            // 4. Fetch request, with processResponse set to the following steps given response response:
            fetch()
        }
    }
}

impl HTMLEmbedElementMethods<crate::DomTypeHolder> for HTMLEmbedElement {
    fn Src(&self, ) -> DOMString {
        let element = self.upcast::<Element>();
        let attr = &local_name!("src");
        if element.has_attribute(attr) {
            element.get_string_attribute(attr)
        } else {
            DOMString::new()
        }
    }

    make_setter!(SetSrc, "src");

    fn Type(&self, ) -> DOMString {
        let element = self.upcast::<Element>();
        let attr = &local_name!("type");
        if element.has_attribute(attr) {
            element.get_string_attribute(attr)
        } else {
            DOMString::new()
        }
    }

   make_setter!(SetType, "type");

    fn Width(&self, ) -> DOMString {
        let element = self.upcast::<Element>();
        let attr = &local_name!("width");
        if element.has_attribute(attr) {
            element.get_string_attribute(attr)
        } else {
            DOMString::new()
        }
    }

    make_setter!(SetWidth, "width");

    fn Height(&self, ) -> DOMString {
        let element = self.upcast::<Element>();
        let attr = &local_name!("height");
        if element.has_attribute(attr) {
            element.get_string_attribute(attr)
        } else {
            DOMString::new()
        }
    }

    make_setter!(SetHeight, "height");

    fn GetSVGDocument(&self, ) -> Option<DomRoot<<crate::DomTypeHolder as script_bindings::DomTypes>::Document>> {
        todo!()
    }

    fn Align(&self, ) -> DOMString {
        todo!()
    }

    make_setter!(SetAlign, "align");

    fn Name(&self, ) -> DOMString {
        todo!()
    }

    make_setter!(SetName, "name");
}
