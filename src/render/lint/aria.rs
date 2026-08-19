//! ARIA that says nothing: a `role` that is not a role, an `aria-*` attribute
//! ARIA does not define, and an id reference pointing at no element.

use crate::config::CheckConfig;
use crate::error::Lint;

use super::{Check, Cx, Findings, Page};

/// The rule that reports unknown or dangling ARIA.
pub(super) struct Aria;

impl Aria {
    /// Every role WAI-ARIA 1.2 defines.
    const ROLES: &'static [&'static str] = &[
        "alert",
        "alertdialog",
        "application",
        "article",
        "associationlist",
        "associationlistitemkey",
        "associationlistitemvalue",
        "banner",
        "blockquote",
        "button",
        "caption",
        "cell",
        "checkbox",
        "code",
        "columnheader",
        "combobox",
        "comment",
        "complementary",
        "contentinfo",
        "definition",
        "deletion",
        "dialog",
        "directory",
        "document",
        "emphasis",
        "feed",
        "figure",
        "form",
        "generic",
        "grid",
        "gridcell",
        "group",
        "heading",
        "img",
        "insertion",
        "link",
        "list",
        "listbox",
        "listitem",
        "log",
        "main",
        "mark",
        "marquee",
        "math",
        "menu",
        "menubar",
        "menuitem",
        "menuitemcheckbox",
        "menuitemradio",
        "meter",
        "navigation",
        "none",
        "note",
        "option",
        "paragraph",
        "presentation",
        "progressbar",
        "radio",
        "radiogroup",
        "region",
        "row",
        "rowgroup",
        "rowheader",
        "scrollbar",
        "search",
        "searchbox",
        "separator",
        "slider",
        "spinbutton",
        "status",
        "strong",
        "subscript",
        "suggestion",
        "superscript",
        "switch",
        "tab",
        "table",
        "tablist",
        "tabpanel",
        "term",
        "textbox",
        "time",
        "timer",
        "toolbar",
        "tooltip",
        "tree",
        "treegrid",
        "treeitem",
    ];

    /// The role vocabularies ARIA's extension modules add, accepted by prefix
    /// rather than enumerated.
    const NAMESPACES: &'static [&'static str] = &["doc-", "graphics-"];

    /// Every state and property WAI-ARIA 1.2 defines, without the `aria-`
    /// prefix every one of them carries.
    const ATTRS: &'static [&'static str] = &[
        "activedescendant",
        "atomic",
        "autocomplete",
        "braillelabel",
        "brailleroledescription",
        "busy",
        "checked",
        "colcount",
        "colindex",
        "colindextext",
        "colspan",
        "controls",
        "current",
        "describedby",
        "description",
        "details",
        "disabled",
        "dropeffect",
        "errormessage",
        "expanded",
        "flowto",
        "grabbed",
        "haspopup",
        "hidden",
        "invalid",
        "keyshortcuts",
        "label",
        "labelledby",
        "level",
        "live",
        "modal",
        "multiline",
        "multiselectable",
        "orientation",
        "owns",
        "placeholder",
        "posinset",
        "pressed",
        "readonly",
        "relevant",
        "required",
        "roledescription",
        "rowcount",
        "rowindex",
        "rowindextext",
        "rowspan",
        "selected",
        "setsize",
        "sort",
        "valuemax",
        "valuemin",
        "valuenow",
        "valuetext",
    ];

    /// The attributes whose value is one or more ids of elements on this page,
    /// and so can dangle.
    const IDREFS: &'static [&'static str] = &[
        "aria-activedescendant",
        "aria-controls",
        "aria-describedby",
        "aria-details",
        "aria-errormessage",
        "aria-flowto",
        "aria-labelledby",
        "aria-owns",
    ];

    /// Whether `role` names something ARIA knows. A role attribute may hold a
    /// fallback chain (`role="doc-chapter section"`), so this judges one token.
    fn role(name: &str) -> bool {
        Self::ROLES.contains(&name)
            || Self::NAMESPACES
                .iter()
                .any(|namespace| name.starts_with(namespace))
    }
}

impl Check for Aria {
    fn enabled(&self, config: &CheckConfig) -> bool {
        config.aria.on()
    }

    fn check(&self, page: &Page, _cx: &Cx<'_>, found: &mut Findings<'_>) {
        for (roles, span) in &page.roles {
            for role in roles.split_ascii_whitespace() {
                if !Self::role(role) {
                    found.push(*span, Lint::Role(role.to_owned()));
                }
            }
        }
        for (attr, value, span) in &page.aria {
            let Some(name) = attr.strip_prefix("aria-") else {
                continue;
            };
            if !Self::ATTRS.contains(&name) {
                found.push(*span, Lint::Attr(attr.clone()));
                continue;
            }
            if !Self::IDREFS.contains(&attr.as_str()) {
                continue;
            }
            for id in value.split_ascii_whitespace() {
                if !page.has(id) {
                    found.push(
                        *span,
                        Lint::Idref {
                            attr: attr.clone(),
                            id: id.to_owned(),
                        },
                    );
                }
            }
        }
    }
}
