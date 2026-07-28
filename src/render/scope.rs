//! Confining a stylesheet to one subtree.
//!
//! An inlined SVG's `<style>` is an ordinary page stylesheet: Illustrator's
//! `.st0{fill:#231F20}` repaints every `.st0` on the page, not just the icon's.
//! [`Scoped`] rewrites each style rule so it can only match inside the element
//! that carries the scope, leaving at-rules that hold no selectors alone.
//!
//! This is a scanner rather than a lightningcss pass on purpose: lightningcss
//! sits behind the `css` cargo feature, and a page whose markup differed
//! between the default and `slim` builds would be a worse problem than the leak
//! it fixes.
//!
//! The rewrite is textual because CSS is text; nothing here builds markup. It
//! preserves whatever it does not need to change (comments, whitespace, the
//! declarations themselves) so a stylesheet stays recognisable in the output.

/// The at-rules whose body holds style rules, and so must be descended into.
/// Everything else (`@keyframes`, `@font-face`, `@property`, `@page`) either
/// holds declarations or names its own thing, and scoping it would break it.
const GROUPING: &[&str] = &["media", "supports", "container", "layer", "scope"];

/// A stylesheet rewriter that confines every rule to one subtree.
pub(crate) struct Scoped {
    /// The selector each rule is confined to. Wrapped in `:where()`, which
    /// contributes no specificity, so confining a rule does not also let it
    /// outrank the page's own CSS.
    scope: String,
}

impl Scoped {
    /// Confine to the elements carrying `attribute="value"`.
    pub(crate) fn attribute(attribute: &str, value: &str) -> Self {
        Self {
            scope: format!(":where([{attribute}=\"{value}\"])"),
        }
    }

    /// `css` with every style rule's selector confined to the scope.
    pub(crate) fn stylesheet(&self, css: &str) -> String {
        let mut out = String::with_capacity(css.len() + css.len() / 4);
        self.rules(css, &mut out);
        out
    }

    /// Copy the rules in `css` into `out`, confining each style rule's selector
    /// and descending into the at-rules that contain style rules.
    fn rules(&self, css: &str, out: &mut String) {
        let src = Css(css);
        let bytes = css.as_bytes();
        let (mut start, mut i) = (0, 0);
        while i < bytes.len() {
            if let Some(next) = src.skip(i) {
                i = next;
                continue;
            }
            match bytes[i] {
                // A statement at-rule (`@import ..;`, `@layer a, b;`) or a
                // stray semicolon: nothing to confine, nothing to descend into.
                b';' => {
                    out.push_str(&css[start..=i]);
                    i += 1;
                    start = i;
                }
                b'{' => {
                    // An unterminated block is a stylesheet we cannot read;
                    // copy the rest verbatim rather than guess where it ended
                    // and silently drop a declaration.
                    let Some(end) = src.block(i) else { break };
                    self.rule(&css[start..i], &css[i + 1..end - 1], out);
                    i = end;
                    start = i;
                }
                _ => i += 1,
            }
        }
        // Trailing whitespace, comments, or an unterminated rule: kept as-is,
        // since a stylesheet we cannot parse is the author's to fix, not ours
        // to silently truncate.
        out.push_str(&css[start..]);
    }

    /// Write one `prelude { block }` rule, confined if it is a style rule.
    fn rule(&self, prelude: &str, body: &str, out: &mut String) {
        // Leading whitespace and comments belong before the rule, not inside
        // the selector we are about to rewrite.
        let selectors = &prelude[Css(prelude).lead()..];
        out.push_str(&prelude[..prelude.len() - selectors.len()]);
        match Css(selectors).at_rule() {
            Some(at) => {
                out.push_str(selectors);
                out.push('{');
                if GROUPING.contains(&at) {
                    self.rules(body, out);
                } else {
                    out.push_str(body);
                }
            }
            None => {
                out.push_str(&self.selectors(selectors));
                out.push('{');
                // A nested rule resolves against its parent, which is already
                // confined, so the body needs no rewriting of its own.
                out.push_str(body);
            }
        }
        out.push('}');
    }

    /// A selector list with every selector in it confined to the scope.
    fn selectors(&self, list: &str) -> String {
        let mut out = String::new();
        for (i, selector) in Css(list).commas().into_iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            out.push_str(&self.scope);
            out.push(' ');
            out.push_str(selector.trim());
        }
        out
    }
}

/// A stretch of CSS being scanned by byte index.
///
/// Every scan below has to agree on where a comment and a string end, or a
/// brace or comma inside one splits a rule in half. Hanging them all off one
/// type is what makes that agreement structural instead of three copies of the
/// same two match arms.
#[derive(Clone, Copy)]
struct Css<'a>(&'a str);

impl<'a> Css<'a> {
    /// The index just past the comment or string starting at `i`, or `None`
    /// when `i` starts neither and the caller should read the byte itself.
    fn skip(self, i: usize) -> Option<usize> {
        let bytes = self.0.as_bytes();
        match bytes[i] {
            b'/' if bytes.get(i + 1) == Some(&b'*') => Some(self.comment(i)),
            b'"' | b'\'' => Some(self.string(i)),
            _ => None,
        }
    }

    /// The index just past a `/* .. */` comment starting at `i`, or the end of
    /// input when it is unterminated.
    fn comment(self, i: usize) -> usize {
        self.0[i + 2..]
            .find("*/")
            .map_or(self.0.len(), |end| i + 2 + end + 2)
    }

    /// The index just past the string starting at `i`, honouring backslash
    /// escapes.
    fn string(self, i: usize) -> usize {
        let bytes = self.0.as_bytes();
        let quote = bytes[i];
        let mut j = i + 1;
        while j < bytes.len() {
            match bytes[j] {
                b'\\' => j += 2,
                c if c == quote => return j + 1,
                _ => j += 1,
            }
        }
        self.0.len()
    }

    /// The index just past the `{ .. }` block whose opening brace is at `i`, or
    /// `None` when it is never closed.
    fn block(self, i: usize) -> Option<usize> {
        let bytes = self.0.as_bytes();
        let (mut j, mut depth) = (i, 0usize);
        while j < bytes.len() {
            if let Some(next) = self.skip(j) {
                j = next;
                continue;
            }
            match bytes[j] {
                b'{' => depth += 1,
                b'}' => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(j + 1);
                    }
                }
                _ => {}
            }
            j += 1;
        }
        None
    }

    /// This selector list split on its top-level commas, so a comma inside
    /// `:is(a, b)` or `[title="a,b"]` does not split a selector in half.
    fn commas(self) -> Vec<&'a str> {
        let bytes = self.0.as_bytes();
        let (mut out, mut start, mut i, mut depth) = (Vec::new(), 0, 0, 0usize);
        while i < bytes.len() {
            if let Some(next) = self.skip(i) {
                i = next;
                continue;
            }
            match bytes[i] {
                b'(' | b'[' => depth += 1,
                b')' | b']' => depth = depth.saturating_sub(1),
                b',' if depth == 0 => {
                    out.push(&self.0[start..i]);
                    start = i + 1;
                }
                _ => {}
            }
            i += 1;
        }
        out.push(&self.0[start..]);
        out
    }

    /// The length of the whitespace and comments this prelude opens with, which
    /// belong before the rule rather than inside its selector.
    fn lead(self) -> usize {
        let mut i = 0;
        loop {
            let rest = &self.0[i..];
            let trimmed = rest.trim_start();
            i += rest.len() - trimmed.len();
            if !self.0[i..].starts_with("/*") {
                return i;
            }
            i = self.comment(i);
        }
    }

    /// The at-rule name this prelude opens with, or `None` when it is a
    /// selector list.
    fn at_rule(self) -> Option<&'a str> {
        let rest = self.0.strip_prefix('@')?;
        let end = rest
            .find(|c: char| c.is_whitespace() || c == '(' || c == '{')
            .unwrap_or(rest.len());
        Some(&rest[..end])
    }
}

#[cfg(test)]
mod tests {
    use super::Scoped;

    /// Confine to `[s="1"]`, short enough to keep the expectations readable.
    fn scope(css: &str) -> String {
        Scoped::attribute("s", "1").stylesheet(css)
    }

    /// The scope selector, spelled once so the expectations below read as CSS
    /// rather than as escaping.
    const S: &str = r#":where([s="1"])"#;

    #[test]
    fn confines_a_plain_rule() {
        assert_eq!(scope(".st0{fill:red}"), format!("{S} .st0{{fill:red}}"));
    }

    #[test]
    fn confines_every_selector_in_a_list() {
        assert_eq!(
            scope(".a, .b > c{fill:red}"),
            format!("{S} .a,{S} .b > c{{fill:red}}")
        );
    }

    #[test]
    fn a_comma_inside_a_selector_does_not_split_it() {
        // The comma belongs to `:is()`, not to the list.
        assert_eq!(
            scope(":is(.a, .b) c{fill:red}"),
            format!("{S} :is(.a, .b) c{{fill:red}}")
        );
        assert_eq!(
            scope(r#"[title="a,b"]{fill:red}"#),
            format!("{S} [title=\"a,b\"]{{fill:red}}")
        );
    }

    #[test]
    fn descends_into_grouping_at_rules() {
        assert_eq!(
            scope("@media (min-width:1px){.a{fill:red}}"),
            format!("@media (min-width:1px){{{S} .a{{fill:red}}}}")
        );
    }

    #[test]
    fn leaves_keyframes_alone() {
        // `from`/`to` are keyframe selectors, not element selectors; confining
        // them would break the animation rather than scope it.
        let css = "@keyframes spin{from{opacity:0}to{opacity:1}}";
        assert_eq!(scope(css), css);
        let css = "@font-face{font-family:x;src:url(a.woff2)}";
        assert_eq!(scope(css), css);
    }

    #[test]
    fn passes_statement_at_rules_through() {
        assert_eq!(
            scope("@import url(x.css);.a{fill:red}"),
            format!("@import url(x.css);{S} .a{{fill:red}}")
        );
    }

    #[test]
    fn a_nested_rule_rides_on_its_parent() {
        // The parent is confined, and a nested selector resolves against it, so
        // rewriting the inner one too would confine it twice.
        assert_eq!(
            scope(".a{color:red;.b{color:blue}}"),
            format!("{S} .a{{color:red;.b{{color:blue}}}}")
        );
    }

    #[test]
    fn keeps_comments_out_of_the_selector() {
        assert_eq!(
            scope("/* c */.a{fill:red}"),
            format!("/* c */{S} .a{{fill:red}}")
        );
    }

    #[test]
    fn a_brace_inside_a_string_is_not_a_block() {
        assert_eq!(
            scope(r#".a{content:"}"}"#),
            format!("{S} .a{{content:\"}}\"}}")
        );
    }

    #[test]
    fn leaves_an_empty_or_unparseable_sheet_intact() {
        assert_eq!(scope(""), "");
        assert_eq!(scope("   \n"), "   \n");
        // Unterminated: kept verbatim rather than half-rewritten. The rule never
        // took effect anyway, and guessing where it ended would drop a
        // declaration.
        assert_eq!(scope(".a{fill:red"), ".a{fill:red");
    }
}
