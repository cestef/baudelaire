//! HTML5 markup re-spelled as XHTML, for the one consumer that needs XML:
//! an EPUB content document is parsed by an XML parser, and typst-html leaves
//! a void element unclosed.

/// Serialized markup, rewritten so an XML parser accepts it.
///
/// A scanner rather than a parse: the input is this build's own serializer
/// output, where every attribute is double-quoted and every `<` in text is
/// already escaped, so a start tag is the one shape that has to be recognized.
pub(super) struct Xhtml;

impl Xhtml {
    /// Elements HTML5 writes with no closing tag, which XML has no notion of.
    const VOID: [&'static str; 13] = [
        "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "source",
        "track", "wbr",
    ];

    /// Elements whose contents are raw text: `<` inside one is not markup, so
    /// the scan steps over them whole.
    const RAW: [&'static str; 2] = ["script", "style"];

    pub(super) fn of(html: &str) -> String {
        let bytes = html.as_bytes();
        let mut out = String::with_capacity(html.len() + 16);
        let mut i = 0;
        while i < bytes.len() {
            let Some(next) = html[i..].find('<') else {
                out.push_str(&html[i..]);
                break;
            };
            let at = i + next;
            out.push_str(&html[i..at]);
            i = match Self::comment(html, at) {
                Some(end) => {
                    out.push_str(&html[at..end]);
                    end
                }
                None => Self::tag(html, at, &mut out),
            };
        }
        out
    }

    /// The end of the comment or doctype opening at `at`, or `None` when what
    /// opens there is an element.
    fn comment(html: &str, at: usize) -> Option<usize> {
        let rest = html.get(at..)?;
        if let Some(body) = rest.strip_prefix("<!--") {
            return Some(body.find("-->").map_or(html.len(), |end| at + 4 + end + 3));
        }
        rest.starts_with("<!")
            .then(|| rest.find('>').map_or(html.len(), |end| at + end + 1))
    }

    /// Copy the tag opening at `at` into `out`, closing it when it is a void
    /// element and stepping over a raw-text element's contents; the offset just
    /// past what was copied.
    fn tag(html: &str, at: usize, out: &mut String) -> usize {
        let Some(end) = Self::close(html, at) else {
            out.push_str(&html[at..]);
            return html.len();
        };
        let tag = &html[at..end];
        let name = Self::name(tag);
        if Self::VOID
            .iter()
            .any(|void| void.eq_ignore_ascii_case(name))
            && !tag.trim_end_matches('>').trim_end().ends_with('/')
        {
            out.push_str(tag.trim_end_matches('>').trim_end());
            out.push_str(" />");
        } else {
            out.push_str(tag);
        }
        if Self::RAW.iter().any(|raw| raw.eq_ignore_ascii_case(name)) && !tag.ends_with("/>") {
            let closing = format!("</{name}");
            if let Some(found) = html[end..].find(&closing) {
                out.push_str(&html[end..end + found]);
                return end + found;
            }
        }
        end
    }

    /// The offset just past the `>` that ends the tag opening at `at`, honoring
    /// the double-quoted attribute values a `>` may sit inside.
    fn close(html: &str, at: usize) -> Option<usize> {
        let mut quoted = false;
        for (offset, c) in html[at..].char_indices() {
            match c {
                '"' => quoted = !quoted,
                '>' if !quoted => return Some(at + offset + 1),
                _ => {}
            }
        }
        None
    }

    /// The tag's element name, empty for a closing tag or a stray `<`.
    fn name(tag: &str) -> &str {
        let rest = tag.trim_start_matches('<');
        let end = rest
            .find(|c: char| c.is_ascii_whitespace() || c == '>' || c == '/')
            .unwrap_or(rest.len());
        &rest[..end]
    }
}

#[cfg(test)]
mod tests {
    use super::Xhtml;

    #[test]
    fn a_void_element_is_closed() {
        assert_eq!(
            Xhtml::of(r#"<p>a<br>b<img src="/x.png" alt="y"></p>"#),
            r#"<p>a<br />b<img src="/x.png" alt="y" /></p>"#
        );
        assert_eq!(Xhtml::of("<hr/>"), "<hr/>", "already closed");
        assert_eq!(Xhtml::of("<hr />"), "<hr />");
    }

    /// `>` is legal unescaped inside an attribute value, so the scan cannot
    /// take the first one it sees as the end of the tag.
    #[test]
    fn a_gt_inside_an_attribute_is_not_the_end_of_the_tag() {
        assert_eq!(
            Xhtml::of(r#"<img alt="a > b" src="/x.png">"#),
            r#"<img alt="a > b" src="/x.png" />"#
        );
    }

    /// Raw text is not markup: a `<` in a stylesheet is a stylesheet's `<`.
    #[test]
    fn raw_text_is_left_alone() {
        let style = "<style>a{content:\"<br>\"}</style><br>";
        assert_eq!(Xhtml::of(style), "<style>a{content:\"<br>\"}</style><br />");
    }

    #[test]
    fn ordinary_markup_survives_unchanged() {
        for html in [
            "<p>plain</p>",
            "<p>&lt;not a tag&gt;</p>",
            "<!-- a > comment --><p>x</p>",
            "<ul><li>a</li><li>b</li></ul>",
            "café <em>théâtre</em>",
        ] {
            assert_eq!(Xhtml::of(html), html);
        }
    }
}
