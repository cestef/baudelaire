//! Plain-text extraction from rendered HTML, and the reading estimate taken
//! from a page's typst source before it is rendered at all.
//!
//! Shared by processors that index or republish page prose (search, llms).
//! A small tag-aware scanner, deliberately *not* a structure-aware rewrite
//! (that is the render layer's job on the typed DOM) and not a full HTML parser.
//! It drops tags and the raw contents of `script`/`style`, decodes the five
//! predefined entities, and collapses runs of whitespace.
//!
//! Extraction is scoped to the page's `<main>` region when it has one, so site
//! chrome (header, sidebar, footer) never pollutes the prose; otherwise every
//! page would index the same navigation text and search relevance collapses.

/// The predefined HTML/XML entities as `(char, name)`, read when decoding
/// `&name;` back to its character during extraction. Escaping the other way is
/// the markup builder's job (`engine/xml.rs`), so there is one escaping surface.
const ENTITIES: &[(char, &str)] = &[
    ('<', "lt"),
    ('>', "gt"),
    ('"', "quot"),
    ('\'', "apos"),
    ('&', "amp"),
];

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
    /// whitespace in a single forward pass writing once into the output.
    fn scan(html: &str) -> String {
        let bytes = html.as_bytes();
        let mut out = String::with_capacity(html.len() / 2);
        // Deferred separator: a run of whitespace or a tag boundary emits at
        // most one space, and only once a following word is written (so leading
        // and trailing whitespace vanish for free).
        let mut gap = false;
        let mut i = 0;
        while i < bytes.len() {
            match bytes[i] {
                b'<' => {
                    // skip a raw element's body (`<script>`/`<style>`) wholesale.
                    if let Some(tag) = Self::raw_element(&html[i..])
                        && let Some(close) = Self::find_close(bytes, i + 1, tag.as_bytes())
                    {
                        i = close;
                    }
                    match html[i..].find('>') {
                        Some(gt) => i += gt + 1,
                        None => break,
                    }
                    gap = true;
                }
                b'&' => {
                    if let Some((ch, len)) = Self::entity(&html[i..]) {
                        Self::push_char(&mut out, &mut gap, ch);
                        i += len;
                    } else {
                        Self::push_char(&mut out, &mut gap, '&');
                        i += 1;
                    }
                }
                b if b.is_ascii_whitespace() => {
                    gap = true;
                    i += 1;
                }
                _ => {
                    // copy a run of plain content at once. stopping only at ASCII
                    // markers keeps the slice on a UTF-8 boundary (multi-byte
                    // scalars never contain these bytes).
                    let start = i;
                    while i < bytes.len()
                        && !matches!(bytes[i], b'<' | b'&')
                        && !bytes[i].is_ascii_whitespace()
                    {
                        i += 1;
                    }
                    Self::push_str(&mut out, &mut gap, &html[start..i]);
                }
            }
        }
        out
    }

    /// Emit `s` after a pending word gap, unless it would be leading whitespace.
    fn push_str(out: &mut String, gap: &mut bool, s: &str) {
        if *gap && !out.is_empty() {
            out.push(' ');
        }
        *gap = false;
        out.push_str(s);
    }

    /// [`Text::push_str`] for a single decoded character.
    fn push_char(out: &mut String, gap: &mut bool, ch: char) {
        if *gap && !out.is_empty() {
            out.push(' ');
        }
        *gap = false;
        out.push(ch);
    }

    /// The name of a raw-text element (`script`/`style`) opening at `tag`, whose
    /// contents must be skipped rather than indexed. Case-insensitive without
    /// allocating (HTML tag names are ASCII).
    fn raw_element(tag: &str) -> Option<&'static str> {
        let b = tag.as_bytes();
        if b.len() >= 7 && b[..7].eq_ignore_ascii_case(b"<script") {
            Some("script")
        } else if b.len() >= 6 && b[..6].eq_ignore_ascii_case(b"<style") {
            Some("style")
        } else {
            None
        }
    }

    /// The byte offset of `</tag` at or after `from`, matched case-insensitively
    /// without copying the haystack. HTML tag names are case-insensitive, so
    /// `</SCRIPT>` closes a `<script>` skip.
    fn find_close(hay: &[u8], from: usize, tag: &[u8]) -> Option<usize> {
        let mut i = from;
        while i + 2 + tag.len() <= hay.len() {
            if hay[i] == b'<'
                && hay[i + 1] == b'/'
                && hay[i + 2..i + 2 + tag.len()].eq_ignore_ascii_case(tag)
            {
                return Some(i);
            }
            i += 1;
        }
        None
    }

    /// Decode one predefined entity at the start of `s` (which begins with `&`),
    /// returning the character and the byte length consumed, or `None` for a bare
    /// `&`. Consuming the whole entity in one step means `&amp;lt;` decodes to a
    /// literal `&lt;`, without ordering `&amp;` last as a replace-based decoder must.
    fn entity(s: &str) -> Option<(char, usize)> {
        if s.starts_with("&#39;") {
            return Some(('\'', 5));
        }
        let after = s.as_bytes().get(1..)?;
        ENTITIES.iter().find_map(|&(ch, name)| {
            let n = name.len();
            (after.len() > n && after[..n] == *name.as_bytes() && after[n] == b';')
                .then_some((ch, n + 2))
        })
    }
}

/// How long a page takes to read: its word count, and the minutes that implies.
///
/// Counted from the page's *typst source*, not its rendered HTML, because that
/// is the only version available when a template is handed its page: the render
/// has not happened yet, and a listing entry is built earlier still. The cost is
/// that it is an estimate, which a reading time is anyway.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Reading {
    pub words: usize,
    pub minutes: usize,
}

impl Reading {
    /// Words per minute. The figure every other generator uses for prose, and
    /// the one a reader has been calibrated against by every "6 min read" badge
    /// they have seen.
    const WPM: usize = 200;

    /// Estimate `body`, a page's typst source.
    ///
    /// Code lines are dropped: in typst markup a leading `#` starts code, so an
    /// `#import` or a `#let` is machinery rather than prose, and counting it
    /// would inflate a short page most. Everything else is counted as prose,
    /// markup and all: an inline `#emph[word]` is one word, which is what it
    /// reads as.
    pub fn of(body: &str) -> Self {
        let words = body
            .lines()
            .map(str::trim_start)
            .filter(|line| !line.starts_with('#') && !line.starts_with("//"))
            .flat_map(str::split_whitespace)
            .filter(|word| word.chars().any(char::is_alphanumeric))
            .count();
        Self {
            words,
            minutes: words.div_ceil(Self::WPM),
        }
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

    /// Prose counts, machinery does not, and the minutes round up so a page
    /// that takes any time at all never reads as zero.
    #[test]
    fn reading_counts_prose_and_skips_typst_code() {
        use super::Reading;
        let body = "#import \"/templates/theme.typ\": callout\n\
                    #let x = 1\n\
                    // a comment\n\
                    \n\
                    = A heading\n\
                    Three plain words, plus #emph[one] more.\n";
        // The heading contributes `A heading` (the bare `=` is punctuation, not
        // a word), and the sentence six, counting `#emph[one]` as the one word
        // it reads as.
        assert_eq!(Reading::of(body).words, 8);
        assert_eq!(Reading::of(body).minutes, 1);
        assert_eq!(Reading::of("").words, 0);
        assert_eq!(Reading::of("").minutes, 0, "nothing takes no time");
        let long = "word ".repeat(401);
        assert_eq!(Reading::of(&long).minutes, 3, "401 words rounds up to 3");
    }
}
