//! Rewrites typst's inline syntax-highlight colours as CSS classes.
//!
//! typst-html bakes a highlight theme's palette into the markup, one
//! `style="color: #rrggbb"` per token span, and offers no way to emit classes
//! instead. A colour frozen at build time cannot follow a light/dark toggle at
//! runtime, so a themed site had to author a `.tmTheme` whose colours are
//! *meaningless sentinels* and win them back with
//! `pre code [style*="e5d004"] { color: var(--kw) !important }`: an
//! attribute-substring selector fighting an inline style.
//!
//! Enabled by the presence of a `html { highlight { .. } }` block, this replaces
//! that colour with a class and leaves the palette to the stylesheet, where it
//! belongs. Only spans inside a `<pre>` are touched: an inline colour in prose
//! is an author's `#text(fill: ..)` and means what it says.

use typst_html::{HtmlDocument, HtmlElement, attr, tag};

use crate::config::{Config, HighlightConfig};

use super::{AttrsExt, Cx, DocumentExt, ElementExt, Transform};

/// The [`Transform`] that turns highlight colours into classes.
pub(super) struct Highlight;

impl Transform for Highlight {
    fn enabled(&self, config: &Config) -> bool {
        config.html.highlight.enabled
    }

    fn apply(&self, doc: &mut HtmlDocument, cx: &mut Cx<'_>) {
        let highlight = &cx.config.html.highlight;
        doc.walk(|element| {
            if element.tag != tag::pre {
                return;
            }
            // The outer walk reaches these descendants again on its own, which
            // costs a second look and changes nothing: a span rewritten once no
            // longer carries a colour to rewrite.
            element.walk(&mut |span| Self::reclass(span, highlight));
        });
    }
}

impl Highlight {
    /// Move one span's colour out of its `style` and into a class.
    ///
    /// Any other declaration in the same `style` survives: a theme may set
    /// `font-style` or `font-weight` on a scope, and those are not the
    /// stylesheet's to own the way a colour is.
    fn reclass(element: &mut HtmlElement, config: &HighlightConfig) {
        let Some(style) = element.attrs.get(attr::style).map(ToString::to_string) else {
            return;
        };
        let Some((colour, rest)) = Self::split(&style) else {
            return;
        };
        let class = config.class(&colour);
        match element.attrs.get(attr::class).map(ToString::to_string) {
            // An authored class stays; the highlight class joins it.
            Some(existing) => element
                .attrs
                .set(attr::class, &format!("{existing} {class}")),
            None => element.attrs.set(attr::class, &class),
        }
        match rest.is_empty() {
            true => element.attrs.remove(attr::style),
            false => element.attrs.set(attr::style, &rest),
        }
    }

    /// Split a `style` into its `color` value and everything else, or `None`
    /// when it declares no colour. The remainder keeps its authored order and
    /// spacing-insensitive form, rebuilt as `a: b; c: d`.
    fn split(style: &str) -> Option<(String, String)> {
        let mut colour = None;
        let mut rest: Vec<&str> = Vec::new();
        for declaration in style.split(';').map(str::trim).filter(|d| !d.is_empty()) {
            match declaration.split_once(':') {
                Some((property, value)) if property.trim().eq_ignore_ascii_case("color") => {
                    colour = Some(value.trim().to_owned());
                }
                _ => rest.push(declaration),
            }
        }
        Some((colour?, rest.join("; ")))
    }
}

#[cfg(test)]
mod tests {
    use super::Highlight;
    use crate::config::HighlightConfig;

    fn config(scopes: &[(&str, &str)]) -> HighlightConfig {
        HighlightConfig {
            enabled: true,
            scopes: scopes
                .iter()
                .map(|(n, c)| ((*n).to_owned(), (*c).to_owned()))
                .collect(),
        }
    }

    #[test]
    fn a_colour_splits_out_of_its_style() {
        assert_eq!(
            Highlight::split("color: #e5d005"),
            Some(("#e5d005".into(), String::new()))
        );
        // A theme that also italicizes a scope keeps that: only the colour is
        // the stylesheet's to own.
        assert_eq!(
            Highlight::split("font-style: italic; color:#abc; font-weight: 700"),
            Some(("#abc".into(), "font-style: italic; font-weight: 700".into()))
        );
        // Nothing to do without a colour.
        assert_eq!(Highlight::split("font-style: italic"), None);
        assert_eq!(Highlight::split(""), None);
    }

    /// A configured scope names the class; anything else falls back to the hex,
    /// which still beats matching on a style substring.
    #[test]
    fn a_named_scope_beats_the_hex_fallback() {
        let config = config(&[("keyword", "#e5d004"), ("string", "#e5d002")]);
        assert_eq!(config.class("#e5d004"), "sx-keyword");
        assert_eq!(config.class("#e5d002"), "sx-string");
        assert_eq!(config.class("#123456"), "sx-123456");
        // A `.tmTheme` may spell its hex either case; the class must not depend
        // on which, or half a palette silently misses its name.
        assert_eq!(config.class("#E5D004"), "sx-keyword");
    }

    #[test]
    fn an_unconfigured_palette_still_yields_classes() {
        assert_eq!(config(&[]).class("#e5d005"), "sx-e5d005");
    }
}
