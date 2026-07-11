//! Plain-text extraction from rendered HTML.
//!
//! Shared by processors that index or republish page prose (search, llms).
//! A small tag-aware scanner — deliberately *not* a structure-aware rewrite
//! (that is the render layer's job on the typed DOM) and not a full HTML parser.
//! It drops tags and the raw contents of `script`/`style`, decodes the five
//! predefined entities, and collapses runs of whitespace.
//!
//! Extraction is scoped to the page's `<main>` region when it has one, so site
//! chrome (header, sidebar, footer) never pollutes the prose — otherwise every
//! page would index the same navigation text and search relevance collapses.

/// The predefined HTML/XML entities as `(char, name)` — the single source both
/// escaping (`char` → `&name;`) and decoding (`&name;` → `char`) read. `amp` is
/// last so decoding it last leaves a literal like `&amp;lt;` intact.
const ENTITIES: &[(char, &str)] = &[
    ('<', "lt"),
    ('>', "gt"),
    ('"', "quot"),
    ('\'', "apos"),
    ('&', "amp"),
];

/// Escapes a string for safe inclusion in HTML/XML text or an attribute value.
pub(super) struct Escaped<'a>(pub(super) &'a str);

impl std::fmt::Display for Escaped<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for c in self.0.chars() {
            match ENTITIES.iter().find(|(ch, _)| *ch == c) {
                Some((_, name)) => write!(f, "&{name};")?,
                None => f.write_str(c.encode_utf8(&mut [0; 4]))?,
            }
        }
        Ok(())
    }
}

pub struct Text;

impl Text {
    pub fn extract(html: &str) -> String {
        Self::scan(Self::main(html))
    }

    /// The inner HTML of the first `<main>` element, or the whole document when
    /// there is none. Keeps indexing focused on primary content.
    fn main(html: &str) -> &str {
        let Some(open) = html.find("<main") else {
            return html;
        };
        let Some(gt) = html[open..].find('>') else {
            return html;
        };
        let start = open + gt + 1;
        match html[start..].find("</main>") {
            Some(end) => &html[start..start + end],
            None => &html[start..],
        }
    }

    /// Strip tags and raw `script`/`style` bodies, decode entities, and collapse
    /// whitespace over `html`.
    fn scan(html: &str) -> String {
        let mut raw = String::with_capacity(html.len() / 2);
        let bytes = html.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] == b'<' {
                // Skip the raw body of a <script>/<style> element wholesale.
                if let Some(tag) = Self::raw_element(&html[i..]) {
                    let close = format!("</{tag}");
                    if let Some(end) = Self::find_ci(&html[i + 1..], &close) {
                        i += 1 + end;
                    }
                }
                match html[i..].find('>') {
                    Some(gt) => i += gt + 1,
                    None => break,
                }
                raw.push(' ');
            } else {
                let end = html[i..].find('<').map_or(html.len(), |gt| i + gt);
                raw.push_str(&Self::decode(&html[i..end]));
                i = end;
            }
        }
        raw.split_whitespace().collect::<Vec<_>>().join(" ")
    }

    /// The name of a raw-text element (`script`/`style`) opening at `tag`, whose
    /// contents must be skipped rather than indexed.
    fn raw_element(tag: &str) -> Option<&'static str> {
        let head = tag.get(..7).unwrap_or(tag).to_ascii_lowercase();
        if head.starts_with("<script") {
            Some("script")
        } else if head.starts_with("<style") {
            Some("style")
        } else {
            None
        }
    }

    /// ASCII-case-insensitive substring search — the single case rule shared by
    /// open- and close-tag matching. `needle` must be lowercase; the haystack is
    /// lowercased for the match (ASCII lowercasing preserves byte length, so the
    /// returned offset is valid in the original). HTML tag names are
    /// case-insensitive, so `</SCRIPT>` must close a `<script>` skip.
    fn find_ci(haystack: &str, needle: &str) -> Option<usize> {
        haystack.to_ascii_lowercase().find(needle)
    }

    /// Decode the predefined entities from the shared [`ENTITIES`] table, plus
    /// the numeric apostrophe alias `&#39;`. `&amp;` is decoded last (it is last
    /// in the table) so a literal like `&amp;lt;` does not turn into `<`.
    fn decode(s: &str) -> String {
        let mut out = s.replace("&#39;", "'");
        for (ch, name) in ENTITIES {
            out = out.replace(&format!("&{name};"), ch.encode_utf8(&mut [0; 4]));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::Text;

    #[test]
    fn strips_tags_scripts_and_decodes_entities() {
        let html = "<h1>Hello</h1><script>ignore()</script><p>a &amp; b &lt;c&gt;</p>";
        assert_eq!(Text::extract(html), "Hello a & b <c>");
    }

    #[test]
    fn skips_uppercase_script_and_style_bodies() {
        // HTML tag names are case-insensitive: an uppercase raw element must
        // still have its body skipped, not indexed as prose.
        let html = "<p>a</p><SCRIPT>leak()</SCRIPT><STYLE>.x{}</STYLE><p>b</p>";
        assert_eq!(Text::extract(html), "a b");
    }

    #[test]
    fn collapses_whitespace_across_tags() {
        assert_eq!(Text::extract("<p>one</p>\n  <p>two</p>"), "one two");
    }

    #[test]
    fn indexes_only_main_content_when_present() {
        let html = "<nav>Home About Contact</nav>\
                    <main><h1>Title</h1><p>real content</p></main>\
                    <footer>copyright</footer>";
        // Chrome outside <main> is excluded from the indexed text.
        assert_eq!(Text::extract(html), "Title real content");
    }
}
