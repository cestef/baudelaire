//! A builder for the line-oriented generated files.
//!
//! The sibling of [`xml`](super::xml) for the four formats that have no grammar
//! to speak of: `_redirects`, `_headers`, `robots.txt` and `llms.txt`. Each is
//! read a line at a time, so a value ends where the line does, and in
//! `_redirects` where the next space begins. Assembled with `writeln!` they
//! were exactly as safe as their values happened to be: a page title carrying a
//! line break put half of itself on a line the host reads as another rule, and
//! nothing in the build said so.
//!
//! Infallible by construction like the XML builder, and for the same reason:
//! the sink is a `String`, whose `fmt::Write` cannot fail. Unlike XML there is
//! nothing to escape *to* here, since the only structure these formats have is
//! the line break itself, so a character a line cannot hold is dropped at the
//! boundary rather than reported. What is dropped is invisible either way; what
//! it would have broken is the whole file.

use std::fmt::{self, Write as _};

/// A generated line-oriented file under construction.
#[derive(Default)]
pub(super) struct Lines(String);

impl Lines {
    /// Open a line. It terminates itself when it goes out of scope, so no
    /// caller can write a value and forget the break after it.
    pub(super) fn line(&mut self) -> Line<'_> {
        Line(&mut self.0)
    }

    /// An empty line: the record separator in `_headers`, and the paragraph
    /// break in `llms.txt`.
    pub(super) fn blank(&mut self) {
        self.0.push('\n');
    }

    /// Finish the file, returning its text.
    pub(super) fn finish(self) -> String {
        self.0
    }
}

/// One line under construction, written part by part.
///
/// Every part says whether it is text this crate authored or a value from the
/// site, and only the second kind is filtered. That distinction is the whole
/// point of the type: a literal `301` and a redirect target are the same bytes
/// to `writeln!` and can never be the same thing here.
pub(super) struct Line<'a>(&'a mut String);

impl Line<'_> {
    /// Fixed text this crate authored: a prefix, a marker, a separator.
    ///
    /// `&'static str` rather than `&str`, for the reason
    /// [`Call::named`](crate::codegen::Call::named) takes one: a literal is the
    /// only thing whose safety can be checked by reading the call, and anything
    /// that is not one is a value and belongs in [`Line::value`].
    pub(super) fn lit(&mut self, text: &'static str) -> &mut Self {
        self.0.push_str(text);
        self
    }

    /// A `name: value` pair, the shape `robots.txt` and `_headers` are written
    /// in.
    pub(super) fn field(&mut self, name: &'static str, value: impl fmt::Display) -> &mut Self {
        self.lit(name).lit(": ").value(value)
    }

    /// A `name: value` pair whose *name* is a value from the site too: a header
    /// the author named in `generate { headers { } }`, where this crate knows
    /// neither half.
    ///
    /// The name is written as a [`word`](Line::word) rather than as a
    /// [`value`](Line::value), because a header name may not carry whitespace
    /// and one that did would put the rest of itself where the host reads the
    /// value.
    pub(super) fn pair(&mut self, name: impl fmt::Display, value: impl fmt::Display) -> &mut Self {
        self.word(name).lit(": ").value(value)
    }

    /// A value from the site, with everything a line cannot carry dropped.
    pub(super) fn value(&mut self, value: impl fmt::Display) -> &mut Self {
        let _ = write!(self.0, "{}", Plain(&value.to_string()));
        self
    }

    /// A value in a space-separated field, which loses its whitespace as well:
    /// a `_redirects` rule is three fields on one line, so a path carrying a
    /// space is read as a rule with a target of `301` and no status at all.
    pub(super) fn word(&mut self, value: impl fmt::Display) -> &mut Self {
        let _ = write!(self.0, "{}", Word(&value.to_string()));
        self
    }
}

/// Terminates the line, so the break is written by the type that knows a line
/// ended rather than by every caller that wrote one.
impl Drop for Line<'_> {
    fn drop(&mut self) {
        self.0.push('\n');
    }
}

/// Displays a value as one line's worth of text: every control character
/// dropped, the line break among them.
///
/// Public to the emitters because a value does not always reach a line through
/// [`Line`]: the content security policy is assembled as one long header value
/// and only then written into `_headers`, and it has to obey the same rule on
/// the way.
pub(super) struct Plain<'a>(pub &'a str);

impl fmt::Display for Plain<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0
            .chars()
            .filter(|c| !c.is_control())
            .try_for_each(|c| f.write_char(c))
    }
}

/// Displays a value as a single whitespace-free field: [`Plain`], and the
/// spaces that would split it into two fields dropped too.
struct Word<'a>(&'a str);

impl fmt::Display for Word<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let squeezed: String = self.0.chars().filter(|c| !c.is_whitespace()).collect();
        write!(f, "{}", Plain(&squeezed))
    }
}

#[cfg(test)]
mod tests {
    use super::{Lines, Plain};

    /// Each line terminates itself, and a blank one is a line of its own.
    #[test]
    fn parts_assemble_into_terminated_lines() {
        let mut lines = Lines::default();
        lines.line().field("User-agent", "*");
        lines.line().lit("Disallow:");
        lines.blank();
        lines.line().lit("# ").value("Title");
        assert_eq!(lines.finish(), "User-agent: *\nDisallow:\n\n# Title\n");
    }

    /// The failure the type exists for: a value carrying a line break used to
    /// end the line early and leave its tail as a record of its own.
    #[test]
    fn a_value_cannot_open_a_line_of_its_own() {
        let mut lines = Lines::default();
        lines.line().field("Sitemap", "https://e.xyz/\nDisallow: /");
        assert_eq!(
            lines.finish(),
            "Sitemap: https://e.xyz/Disallow: /\n",
            "the injected line survived"
        );
    }

    /// A space-separated format loses the spaces inside a field, or the fields
    /// after it shift by one and the status is read off the wrong column.
    #[test]
    fn a_field_of_a_space_separated_line_keeps_its_spaces_out() {
        let mut lines = Lines::default();
        lines.line().word("/old path/").lit(" ").word("/new/");
        assert_eq!(lines.finish(), "/oldpath/ /new/\n");
    }

    /// The same rule, reachable without a line: what the policy is written
    /// through before it becomes one header's value. A space is a value's own
    /// business here, unlike in a field.
    #[test]
    fn a_plain_value_drops_what_a_line_cannot_hold() {
        assert_eq!(Plain("a\r\nb").to_string(), "ab");
        assert_eq!(Plain("'self' https:").to_string(), "'self' https:");
    }
}
