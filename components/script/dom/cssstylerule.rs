/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use std::mem;

use cssparser::{Parser as CssParser, ParserInput as CssParserInput, ToCss};
use dom_struct::dom_struct;
use selectors::parser::{ParseRelative, SelectorList};
use servo_arc::Arc;
use style::selector_parser::SelectorParser;
use style::shared_lock::{Locked, ToCssWithGuard};
use style::stylesheets::{CssRuleType, CssRules, Origin, StyleRule};

use crate::dom::bindings::codegen::Bindings::CSSStyleRuleBinding::CSSStyleRuleMethods;
use crate::dom::bindings::inheritance::Castable;
use crate::dom::bindings::reflector::{DomGlobal, reflect_dom_object};
use crate::dom::bindings::root::{Dom, DomRoot, MutNullableDom};
use crate::dom::bindings::str::DOMString;
use crate::dom::cssgroupingrule::CSSGroupingRule;
use crate::dom::cssrule::SpecificCSSRule;
use crate::dom::cssstyledeclaration::{CSSModificationAccess, CSSStyleDeclaration, CSSStyleOwner};
use crate::dom::cssstylesheet::CSSStyleSheet;
use crate::dom::window::Window;
use crate::script_runtime::CanGc;

#[dom_struct]
pub(crate) struct CSSStyleRule {
    cssgroupingrule: CSSGroupingRule,
    #[ignore_malloc_size_of = "Arc"]
    #[no_trace]
    stylerule: Arc<Locked<StyleRule>>,
    style_decl: MutNullableDom<CSSStyleDeclaration>,
}

impl CSSStyleRule {
    fn new_inherited(
        parent_stylesheet: &CSSStyleSheet,
        stylerule: Arc<Locked<StyleRule>>,
    ) -> CSSStyleRule {
        CSSStyleRule {
            cssgroupingrule: CSSGroupingRule::new_inherited(parent_stylesheet),
            stylerule,
            style_decl: Default::default(),
        }
    }

    #[cfg_attr(crown, allow(crown::unrooted_must_root))]
    pub(crate) fn new(
        window: &Window,
        parent_stylesheet: &CSSStyleSheet,
        stylerule: Arc<Locked<StyleRule>>,
        can_gc: CanGc,
    ) -> DomRoot<CSSStyleRule> {
        reflect_dom_object(
            Box::new(CSSStyleRule::new_inherited(parent_stylesheet, stylerule)),
            window,
            can_gc,
        )
    }

    pub(crate) fn ensure_rules(&self) -> Arc<Locked<CssRules>> {
        let lock = self.cssgroupingrule.shared_lock();
        let mut guard = lock.write();
        self.stylerule
            .write_with(&mut guard)
            .rules
            .get_or_insert_with(|| CssRules::new(vec![], lock))
            .clone()
    }
}

impl SpecificCSSRule for CSSStyleRule {
    fn ty(&self) -> CssRuleType {
        CssRuleType::Style
    }

    fn get_css(&self) -> DOMString {
        let guard = self.cssgroupingrule.shared_lock().read();
        self.stylerule
            .read_with(&guard)
            .to_css_string(&guard)
            .into()
    }
}

impl CSSStyleRuleMethods<crate::DomTypeHolder> for CSSStyleRule {
    // https://drafts.csswg.org/cssom/#dom-cssstylerule-style
    fn Style(&self, can_gc: CanGc) -> DomRoot<CSSStyleDeclaration> {
        self.style_decl.or_init(|| {
            let guard = self.cssgroupingrule.shared_lock().read();
            CSSStyleDeclaration::new(
                self.global().as_window(),
                CSSStyleOwner::CSSRule(
                    Dom::from_ref(self.upcast()),
                    self.stylerule.read_with(&guard).block.clone(),
                ),
                None,
                CSSModificationAccess::ReadWrite,
                can_gc,
            )
        })
    }

    // https://drafts.csswg.org/cssom/#dom-cssstylerule-selectortext
    fn SelectorText(&self) -> DOMString {
        let guard = self.cssgroupingrule.shared_lock().read();
        let stylerule = self.stylerule.read_with(&guard);
        DOMString::from_string(stylerule.selectors.to_css_string())
    }

    // https://drafts.csswg.org/cssom/#dom-cssstylerule-selectortext
    fn SetSelectorText(&self, value: DOMString) {
        let contents = &self
            .cssgroupingrule
            .parent_stylesheet()
            .style_stylesheet()
            .contents;
        // It's not clear from the spec if we should use the stylesheet's namespaces.
        // https://github.com/w3c/csswg-drafts/issues/1511
        let namespaces = contents.namespaces.read();
        let url_data = contents.url_data.read();
        let parser = SelectorParser {
            stylesheet_origin: Origin::Author,
            namespaces: &namespaces,
            url_data: &url_data,
            for_supports_rule: false,
        };
        let mut css_parser = CssParserInput::new(&value);
        let mut css_parser = CssParser::new(&mut css_parser);
        // TODO: Maybe allow setting relative selectors from the OM, if we're in a nested style
        // rule?
        if let Ok(mut s) = SelectorList::parse(&parser, &mut css_parser, ParseRelative::No) {
            // This mirrors what we do in CSSStyleOwner::mutate_associated_block.
            let mut guard = self.cssgroupingrule.shared_lock().write();
            let stylerule = self.stylerule.write_with(&mut guard);
            mem::swap(&mut stylerule.selectors, &mut s);
            self.cssgroupingrule
                .parent_stylesheet()
                .notify_invalidations();
        }
    }
}
