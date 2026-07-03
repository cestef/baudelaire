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

pub(super) struct Text;

impl Text {
    pub(super) fn extract(html: &str) -> String {
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
                    if let Some(end) = html[i + 1..].find(&close) {
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

    /// Decode the five predefined XML/HTML entities. `&amp;` is decoded last so
    /// a literal like `&amp;lt;` does not turn into `<`.
    fn decode(s: &str) -> String {
        s.replace("&lt;", "<")
            .replace("&gt;", ">")
            .replace("&quot;", "\"")
            .replace("&#39;", "'")
            .replace("&apos;", "'")
            .replace("&amp;", "&")
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
