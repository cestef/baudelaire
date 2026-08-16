//! A builder for the line-oriented generated files (`_redirects`, `_headers`,
//! `robots.txt`, `llms.txt`), where a value ends where the line does and so is
//! stripped of anything a line cannot hold.

use std::fmt::{self, Write as _};

/// A generated line-oriented file under construction.
#[derive(Default)]
pub(super) struct Lines(String);

impl Lines {
    /// Open a line; it terminates itself when it goes out of scope.
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
/// site, and only the second kind is filtered.
pub(super) struct Line<'a>(&'a mut String);

impl Line<'_> {
    /// Fixed text this crate authored: a prefix, a marker, a separator.
    ///
    /// `&'static str` rather than `&str`, because a literal is the only thing
    /// whose safety can be checked by reading the call; anything else is a
    /// value and belongs in [`Line::value`].
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
    /// The name is written as a [`word`](Line::word), because a header name may
    /// not carry whitespace and one that did would put the rest of itself where
    /// the host reads the value.
    pub(super) fn pair(&mut self, name: impl fmt::Display, value: impl fmt::Display) -> &mut Self {
        self.word(name).lit(": ").value(value)
    }

    /// A value from the site, with everything a line cannot carry dropped.
    pub(super) fn value(&mut self, value: impl fmt::Display) -> &mut Self {
        let _ = write!(self.0, "{}", Plain(&value.to_string()));
        self
    }

    /// A value inside a Markdown inline link, either half of it. See [`Link`].
    pub(super) fn linked(&mut self, value: impl fmt::Display) -> &mut Self {
        let _ = write!(self.0, "{}", Link(&value.to_string()));
        self
    }

    /// A value in a space-separated field, which loses its whitespace as well:
    /// a `_redirects` rule is three fields on one line, so a path carrying a
    /// space shifts every field after it.
    pub(super) fn word(&mut self, value: impl fmt::Display) -> &mut Self {
        let _ = write!(self.0, "{}", Word(&value.to_string()));
        self
    }
}

impl Drop for Line<'_> {
    fn drop(&mut self) {
        self.0.push('\n');
    }
}

/// Displays a value as one line's worth of text: every control character
/// dropped, the line break among them.
///
/// Public to the emitters because a value does not always reach a line through
/// [`Line`]: the content security policy is assembled as one header value
/// first, and has to obey the same rule on the way.
pub(super) struct Plain<'a>(pub &'a str);

impl fmt::Display for Plain<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0
            .chars()
            .filter(|c| !c.is_control())
            .try_for_each(|c| f.write_char(c))
    }
}

/// Displays a value inside a Markdown inline link: [`Plain`], with the four
/// delimiters that would end the link early escaped; `llms.txt` is Markdown,
/// and both halves of `[text](url)` are site values.
struct Link<'a>(&'a str);

impl fmt::Display for Link<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for c in self.0.chars().filter(|c| !c.is_control()) {
            if matches!(c, '[' | ']' | '(' | ')' | '\\') {
                f.write_char('\\')?;
            }
            f.write_char(c)?;
        }
        Ok(())
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

    #[test]
    fn parts_assemble_into_terminated_lines() {
        let mut lines = Lines::default();
        lines.line().field("User-agent", "*");
        lines.line().lit("Disallow:");
        lines.blank();
        lines.line().lit("# ").value("Title");
        assert_eq!(lines.finish(), "User-agent: *\nDisallow:\n\n# Title\n");
    }

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

    #[test]
    fn a_field_of_a_space_separated_line_keeps_its_spaces_out() {
        let mut lines = Lines::default();
        lines.line().word("/old path/").lit(" ").word("/new/");
        assert_eq!(lines.finish(), "/oldpath/ /new/\n");
    }

    #[test]
    fn a_plain_value_drops_what_a_line_cannot_hold() {
        assert_eq!(Plain("a\r\nb").to_string(), "ab");
        assert_eq!(Plain("'self' https:").to_string(), "'self' https:");
    }
}
