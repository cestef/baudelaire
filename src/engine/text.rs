//! Plain-text extraction from rendered HTML, and the reading estimate taken
//! from a page's typst source before it is rendered at all.
//!
//! Read by the search index, the one processor that needs a page's prose as
//! text. A small tag-aware scanner, deliberately *not* a structure-aware rewrite
//! (that is the render layer's job on the typed DOM) and not a full HTML parser.
//! It drops tags and the raw contents of `script`/`style`, decodes the five
//! predefined entities, and collapses runs of whitespace.
//!
//! Extraction is scoped to one region of the page ([`Region`]): by default the
//! `<main>` landmark, so site chrome (header, sidebar, footer) never pollutes
//! the prose. Otherwise every page indexes the same navigation text and search
//! relevance collapses. A layout that names its content region something else,
//! or that puts chrome *inside* it, says so in config rather than losing.

use std::borrow::Cow;

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

/// Which part of a page is prose: the element its text is taken from, and the
/// elements inside it that are not text at all.
///
/// Both are tag names rather than selectors. A selector engine would be a
/// second HTML parser, and the thing being answered is "which landmark", which
/// a name answers: `main` for the default layout, `article` for one that binds
/// its prose to that, `body` for a page that is nothing but prose.
#[derive(Debug, Clone, Copy)]
pub struct Region<'a> {
    /// The element whose contents are the page's prose, by tag name.
    pub element: &'a str,
    /// Elements dropped wherever they occur inside it, by tag name.
    pub ignore: &'a [String],
}

impl Default for Region<'_> {
    fn default() -> Self {
        Self {
            element: Self::MAIN,
            ignore: &[],
        }
    }
}

impl Region<'_> {
    /// The landmark a page's own prose lives in, and the default everything
    /// falls back to.
    pub const MAIN: &'static str = "main";
}

impl<'a> From<&'a crate::config::RegionConfig> for Region<'a> {
    fn from(config: &'a crate::config::RegionConfig) -> Self {
        Self {
            element: &config.element,
            ignore: &config.ignore,
        }
    }
}

pub struct Text;

impl Text {
    pub fn extract(html: &str, region: Region) -> String {
        Self::scan(Self::region(html, region.element), region.ignore)
    }

    /// The page's prose as *markup*, for a feed that carries each entry in full:
    /// the region, with the elements the site calls chrome removed from inside
    /// it.
    ///
    /// Falls back to `<body>` rather than to the whole document, which is where
    /// this parts company with [`Text::region`]. A document begins
    /// `<!DOCTYPE html><head>`, and a layout that names no region would
    /// otherwise publish its every `<meta>` tag as the entry's body. `<body>` is
    /// the widest thing that is still prose, and typst-html always emits one.
    ///
    /// `<script>` and `<style>` go the way of an ignored element, since a feed
    /// reader runs neither and both would travel as text.
    pub fn prose<'a>(html: &'a str, region: Region<'_>) -> Cow<'a, str> {
        let inner = Self::found(html, region.element)
            .or_else(|| Self::found(html, "body"))
            .unwrap_or(html);
        Self::stripped(inner, region.ignore)
    }

    /// `html` with every element [`Text::skipped`] names removed, contents and
    /// all. Borrowed when there was nothing to remove, which is the common case:
    /// a page whose region holds no chrome copies no bytes.
    ///
    /// An element that never closes is left alone rather than swallowing the
    /// rest of the document, the same call [`Text::scan`] makes.
    fn stripped<'a>(html: &'a str, ignore: &[String]) -> Cow<'a, str> {
        let mut out: Option<String> = None;
        // Where the run not yet copied into `out` begins.
        let mut kept = 0;
        let mut i = 0;
        while let Some(next) = html[i..].find('<') {
            let at = i + next;
            let Some(tag) = Self::skipped(&html[at..], ignore) else {
                i = at + 1;
                continue;
            };
            let Some(close) = Self::closing(html, at + 1 + tag.len(), tag) else {
                i = at + 1;
                continue;
            };
            let end = match html[close..].find('>') {
                Some(gt) => close + gt + 1,
                None => html.len(),
            };
            out.get_or_insert_with(String::new)
                .push_str(&html[kept..at]);
            kept = end;
            i = end;
        }
        match out {
            Some(mut out) => {
                out.push_str(&html[kept..]);
                Cow::Owned(out)
            }
            None => Cow::Borrowed(html),
        }
    }

    /// The inner HTML of the first `element`, or the whole document when there
    /// is none. Keeps a consumer focused on primary content, and leaves a page
    /// that does not have the region counted whole rather than not at all: a 404
    /// page or a bare feed page is prose too.
    ///
    /// That fallback is right for the search index and wrong for a feed, which
    /// is why [`Text::prose`] does not share it.
    pub fn region<'a>(html: &'a str, element: &str) -> &'a str {
        Self::found(html, element).unwrap_or(html)
    }

    /// The inner HTML of the first `element`, or `None` when the document holds
    /// no such element. A name that is empty is every element there is, and so
    /// is the whole document.
    ///
    /// Split out because the two readers want opposite things from a miss: the
    /// search index would rather index a page whole than not at all, and a feed
    /// would rather narrow than publish a `<head>`.
    fn found<'a>(html: &'a str, element: &str) -> Option<&'a str> {
        if element.is_empty() {
            return Some(html);
        }
        let open = Self::opening(html, 0, element)?;
        let gt = html[open..].find('>')?;
        let start = open + gt + 1;
        Some(match Self::closing(html, start, element) {
            Some(end) => &html[start..end],
            None => &html[start..],
        })
    }

    /// Where `<element` opens at or after `from`, as a whole tag name: `<mainly>`
    /// is not a `<main>`, and neither is the `<main-menu>` a component may emit.
    fn opening(html: &str, from: usize, element: &str) -> Option<usize> {
        let bytes = html.as_bytes();
        let mut i = from;
        while let Some(next) = html[i..].find('<') {
            let at = i + next;
            let name = at + 1;
            let end = name + element.len();
            if end <= bytes.len()
                && bytes[name..end].eq_ignore_ascii_case(element.as_bytes())
                && bytes
                    .get(end)
                    .is_some_and(|b| b.is_ascii_whitespace() || *b == b'>' || *b == b'/')
            {
                return Some(at);
            }
            i = at + 1;
        }
        None
    }

    /// Where the `element` opened before `from` closes, counting nesting so an
    /// inner one of the same name does not end the outer.
    fn closing(html: &str, from: usize, element: &str) -> Option<usize> {
        let mut depth = 1usize;
        let mut i = from;
        loop {
            let open = Self::opening(html, i, element);
            let close = Self::find_close(html.as_bytes(), i, element.as_bytes());
            match (open, close) {
                (Some(open), Some(close)) if open < close => {
                    depth += 1;
                    i = open + 1;
                }
                (_, Some(close)) => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(close);
                    }
                    i = close + 1;
                }
                (_, None) => return None,
            }
        }
    }

    /// Strip tags and raw `script`/`style` bodies, decode entities, and collapse
    /// whitespace in a single forward pass writing once into the output.
    ///
    /// An `ignore`d element is skipped whole, contents and all, the way a
    /// `<script>` is: it is chrome that happens to sit inside the prose, and
    /// indexing it would put a sidebar's every link into every page's text.
    fn scan(html: &str, ignore: &[String]) -> String {
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
                    // skip a raw element's body (`<script>`/`<style>`), and any
                    // element the site excludes, wholesale.
                    if let Some(tag) = Self::skipped(&html[i..], ignore)
                        && let Some(close) = Self::closing(html, i + 1 + tag.len(), tag)
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

    /// The name of the element opening at `tag` whose contents are skipped
    /// rather than read: a raw-text element, always; one the site excludes; or
    /// one marked `aria-hidden="true"`.
    ///
    /// That last rule is what keeps a heading's own self link
    /// (`html { anchors { link } }`) out of the prose. It is markup this crate
    /// injects, saying nothing a reader has not just been told by the heading
    /// around it, and its `#` would otherwise be a word in every page's search
    /// index and a stray character in every feed body. Honouring the attribute
    /// rather than the class covers a theme's own decorative markup too, which
    /// `ignore` cannot: a name matches every element that has it.
    ///
    /// Case-insensitive without allocating (HTML tag names are ASCII).
    fn skipped<'a>(tag: &'a str, ignore: &[String]) -> Option<&'a str> {
        const RAW: [&str; 2] = ["script", "style"];
        let name = Self::name(tag)?;
        let excluded = RAW
            .iter()
            .copied()
            .chain(ignore.iter().map(String::as_str))
            .any(|excluded| excluded.eq_ignore_ascii_case(name));
        (excluded || Self::hidden(tag)).then_some(name)
    }

    /// The tag name of the element opening at `tag`, or `None` when what opens
    /// there is not one: a closing tag, a comment, a doctype, a stray `<`.
    fn name(tag: &str) -> Option<&str> {
        let rest = tag.strip_prefix('<')?;
        let end = rest.find(|c: char| c.is_ascii_whitespace() || c == '>' || c == '/')?;
        let name = rest.get(..end).filter(|name| !name.is_empty())?;
        name.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-')
            .then_some(name)
    }

    /// Whether the element opening at `tag` declares itself hidden from
    /// assistive technology. Read off the open tag as written, which is this
    /// crate's own serializer output: a lowercase name, a quoted value.
    fn hidden(tag: &str) -> bool {
        tag.find('>')
            .is_some_and(|gt| tag[..gt].contains(r#"aria-hidden="true""#))
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

/// How long a page takes to read, as the prose words it carries.
///
/// Counted from the page's *source*, not from its rendered HTML, because that is
/// the only version available when a template is handed its page: the render has
/// not happened yet, and a listing entry is built earlier still. The cost is that
/// it is an estimate, which a reading time is anyway.
///
/// Source means the text the author wrote, in the dialect they wrote it: one
/// constructor per dialect, because what counts as machinery is exactly what
/// differs between them, and a page measured by the other one's rule reads as
/// nothing at all.
///
/// Words and not minutes, because the rate is the *site's* and this is measured
/// before a page knows which language it is in: [`Reading::minutes`] applies it
/// where both are in hand. Storing the minutes meant baking a constant into a
/// value carried across the build.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Reading {
    pub words: usize,
}

impl Reading {
    /// Estimate `body`, a page's typst source.
    ///
    /// Code lines are dropped: in typst markup a leading `#` starts code, so an
    /// `#import` or a `#let` is machinery rather than prose, and counting it
    /// would inflate a short page most. Everything else is counted as prose,
    /// markup and all: an inline `#emph[word]` is one word, which is what it
    /// reads as.
    pub fn of(body: &str) -> Self {
        Self::counted(
            body.lines()
                .map(str::trim_start)
                .filter(|line| !line.starts_with('#') && !line.starts_with("//"))
                .map(Self::words)
                .sum(),
        )
    }

    /// Estimate `body`, the markdown an author wrote, rather than the typst it
    /// lowers to.
    ///
    /// It has to be the authored text. Every line the lowering emits begins with
    /// `#` (`#"prose"`, `#heading(level: 2)[`, `#list(`), which is the very
    /// shape [`Reading::of`] reads as machinery, so a lowered page counted as
    /// nothing at all and every markdown page shipped "0 min read".
    ///
    /// The rule is [`Reading::of`]'s, translated: a fenced code block is what a
    /// `#`-line is there, machinery rather than prose, and everything else is
    /// counted with its markup, an inline `**word**` reading as the one word it
    /// is. A heading is prose here, unlike in typst, because `#` opens one
    /// rather than opening code.
    #[cfg(feature = "markdown")]
    pub fn markdown(body: &str) -> Self {
        let mut fence: Option<&str> = None;
        let mut words = 0;
        for line in body.lines() {
            let line = line.trim_start();
            // Inside a block, the only line that matters is the one closing it,
            // and a closing fence is at least as long as the one it closes: the
            // opening run is the needle rather than the whole line.
            if let Some(open) = fence {
                if line.starts_with(open) {
                    fence = None;
                }
                continue;
            }
            match Self::opens(line) {
                Some(open) => fence = Some(open),
                None => words += Self::words(line),
            }
        }
        Self::counted(words)
    }

    /// The fence a line opens a code block with: a run of three or more
    /// backticks or tildes, without whatever info string follows it.
    #[cfg(feature = "markdown")]
    fn opens(line: &str) -> Option<&str> {
        let marker = line.chars().next().filter(|c| *c == '`' || *c == '~')?;
        // Both markers are one byte, so the trimmed difference is the run's
        // length in characters as well as in bytes.
        let run = line.len() - line.trim_start_matches(marker).len();
        if run < 3 {
            return None;
        }
        Some(&line[..run])
    }

    /// The prose words in one line: whitespace-separated runs carrying at least
    /// one alphanumeric, so a bare `=` or `-` marker is punctuation rather than
    /// a word.
    fn words(line: &str) -> usize {
        line.split_whitespace()
            .filter(|word| word.chars().any(char::is_alphanumeric))
            .count()
    }

    /// The minutes this many words imply at `wpm`, rounded up: the one place a
    /// rate is applied, so the two dialects cannot round differently.
    ///
    /// A rate of nothing is refused at config parse, so the division is safe;
    /// the guard is belt and braces against a caller that built one by hand.
    pub fn minutes(self, wpm: usize) -> usize {
        self.words.div_ceil(wpm.max(1))
    }

    /// A word count as a reading estimate.
    fn counted(words: usize) -> Self {
        Self { words }
    }
}

#[cfg(test)]
mod tests {
    use super::{Region, Text};

    /// The default region, for the cases that are about the scanner rather than
    /// about which part of the page it reads.
    fn text(html: &str) -> String {
        Text::extract(html, Region::default())
    }

    #[test]
    fn strips_tags_scripts_and_decodes_entities() {
        let html = "<h1>Hello</h1><script>ignore()</script><p>a &amp; b &lt;c&gt;</p>";
        assert_eq!(text(html), "Hello a & b <c>");
    }

    #[test]
    fn skips_uppercase_script_and_style_bodies() {
        // HTML tag names are case-insensitive: an uppercase raw element must
        // still have its body skipped, not indexed as prose.
        let html = "<p>a</p><SCRIPT>leak()</SCRIPT><STYLE>.x{}</STYLE><p>b</p>";
        assert_eq!(text(html), "a b");
    }

    #[test]
    fn collapses_whitespace_across_tags() {
        assert_eq!(text("<p>one</p>\n  <p>two</p>"), "one two");
    }

    #[test]
    fn indexes_only_main_content_when_present() {
        let html = "<nav>Home About Contact</nav>\
                    <main><h1>Title</h1><p>real content</p></main>\
                    <footer>copyright</footer>";
        // Chrome outside <main> is excluded from the indexed text.
        assert_eq!(text(html), "Title real content");
    }

    /// A layout that binds its prose to something else says so, and the whole
    /// page is still the answer for a page that has no such region.
    #[test]
    fn the_region_is_whatever_the_site_names() {
        let html = "<nav>chrome</nav><article><p>prose</p></article>";
        let article = Region {
            element: "article",
            ..Region::default()
        };
        assert_eq!(Text::extract(html, article), "prose");
        // ...and `main`, which this page does not have, falls back to all of it.
        assert_eq!(text(html), "chrome prose");
        // A region named as nothing is the whole document, deliberately: it is
        // how a site says it has no chrome to keep out.
        let all = Region {
            element: "",
            ..Region::default()
        };
        assert_eq!(Text::extract(html, all), "chrome prose");
    }

    /// A name is a whole tag name: an element that merely starts with it is a
    /// different element, and reading it as the region would index the wrong
    /// half of the page.
    #[test]
    fn a_region_matches_a_whole_tag_name() {
        let html = "<main-menu>chrome</main-menu><main>prose</main>";
        assert_eq!(text(html), "prose");
    }

    /// The region closes at *its* closing tag, not at the first one that looks
    /// like it: a nested element of the same name used to end the scan early and
    /// truncate every page that had one.
    #[test]
    fn a_nested_region_does_not_end_the_outer_one() {
        let html = "<main>before <main>inner</main> after</main><footer>chrome</footer>";
        assert_eq!(text(html), "before inner after");
    }

    /// Chrome a layout puts *inside* its content region: skipped whole, the way
    /// a script is, because a sidebar's links in every page's text is exactly
    /// what scoping to a region exists to prevent.
    #[test]
    fn ignored_elements_are_dropped_from_the_region() {
        let html = "<main><nav>Home About</nav><p>prose</p><aside>related</aside></main>";
        let region = Region {
            element: Region::MAIN,
            ignore: &["nav".to_owned(), "aside".to_owned()],
        };
        assert_eq!(Text::extract(html, region), "prose");
        // ...and nesting is counted here too, so an inner one does not end the
        // skip early and leak the rest of the outer element.
        let nested = "<main><nav>a<nav>b</nav>c</nav><p>prose</p></main>";
        assert_eq!(Text::extract(nested, region), "prose");
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
        assert_eq!(Reading::of(body).minutes(200), 1);
        assert_eq!(Reading::of("").words, 0);
        assert_eq!(Reading::of("").minutes(200), 0, "nothing takes no time");
        let long = "word ".repeat(401);
        assert_eq!(
            Reading::of(&long).minutes(200),
            3,
            "401 words rounds up to 3"
        );
    }

    /// A markdown page is estimated from the markdown, never from the typst it
    /// lowers to: every line of that starts with `#`, so reading the lowered
    /// body counted nothing at all and every `.md` page shipped `0 min read`.
    #[test]
    #[cfg(feature = "markdown")]
    fn reading_counts_markdown_prose_and_skips_its_code_fences() {
        use super::Reading;
        let body = "## A heading\n\
                    \n\
                    Three plain words, plus **one** more.\n\
                    \n\
                    ```rust\n\
                    let a = 1;\n\
                    let b = 2;\n\
                    ```\n\
                    \n\
                    - a list item\n";
        // The heading contributes `A heading` (the `##` is punctuation, not a
        // word), the sentence six, the list item three, and the fence none.
        assert_eq!(Reading::markdown(body).words, 11);
        assert_eq!(Reading::markdown(body).minutes(200), 1);
        // ...and the lowered typst of the very same page reads as nothing,
        // which is what makes the authored text the only thing worth counting.
        let lowered = "#heading(level: 2)[#\"A heading\"]\n\
                       #\"Three plain words, plus \"#strong[#\"one\"]#\" more.\"\n";
        assert_eq!(Reading::of(lowered).words, 0);
        assert_eq!(Reading::markdown("").words, 0);
    }

    /// A fence closes on a run at least as long as the one that opened it, and
    /// an unterminated one runs to the end of the page rather than counting the
    /// rest of it as prose.
    #[test]
    #[cfg(feature = "markdown")]
    fn a_longer_fence_closes_a_shorter_one_and_an_unclosed_fence_ends_the_page() {
        use super::Reading;
        assert_eq!(
            Reading::markdown("```\ncode words here\n````\nprose words\n").words,
            2
        );
        assert_eq!(Reading::markdown("~~~\ncode here\n~~~\nprose\n").words, 1);
        assert_eq!(Reading::markdown("prose\n```\nnever closed\n").words, 1);
        // Two backticks are inline code, not a fence: the line is prose.
        assert_eq!(Reading::markdown("`` a b ``\n").words, 2);
    }

    /// The document a page is: what a full feed slices its entry bodies out of.
    const PAGE: &str = "<!DOCTYPE html><html><head><meta charset=\"utf-8\"></head>\
                        <body><nav>chrome</nav><main><nav>inner</nav>\
                        <p>Real prose.</p></main></body></html>";

    /// A feed body is the region and nothing around it. The search index reads
    /// a page with no region whole; a feed must not, or every entry is the
    /// document, `<head>` and all.
    #[test]
    fn a_feed_body_narrows_where_the_search_index_widens() {
        let region = Region {
            element: "article",
            ignore: &[],
        };
        assert!(
            Text::region(PAGE, "article").starts_with("<!DOCTYPE"),
            "the index counts a page with no region whole"
        );
        let body = Text::prose(PAGE, region);
        assert!(!body.contains("<!DOCTYPE"), "{body}");
        assert!(!body.contains("charset"), "{body}");
        assert!(body.contains("Real prose."), "{body}");
        // The fallback is `<body>`, so the layout's own chrome is still there:
        // narrowing it is what `ignore` is for.
        assert!(body.contains("chrome"), "{body}");
    }

    /// `ignore` names the chrome a layout puts *inside* its region, so it has to
    /// reach the markup a feed carries and not only the text an index reads.
    #[test]
    fn a_feed_body_drops_the_elements_the_site_calls_chrome() {
        let ignore = ["nav".to_owned()];
        let body = Text::prose(
            PAGE,
            Region {
                element: "main",
                ignore: &ignore,
            },
        );
        assert_eq!(body, "<p>Real prose.</p>");
    }

    /// Nothing to drop copies nothing: the common case stays a borrow of the
    /// page that is already in memory.
    #[test]
    fn a_region_holding_no_chrome_is_not_copied() {
        let body = Text::prose(
            PAGE,
            Region {
                element: "main",
                ignore: &[],
            },
        );
        assert!(matches!(body, std::borrow::Cow::Borrowed(_)));
        assert_eq!(body, "<nav>inner</nav><p>Real prose.</p>");
    }

    /// Markup that announces itself as hidden is not prose, in either reader.
    /// A heading's self link is the case this exists for: it says nothing the
    /// heading has not, so its `#` belongs in neither the index nor a feed.
    #[test]
    fn what_is_hidden_from_a_reader_is_hidden_from_both_readers() {
        let html = "<main><h2 id=\"one\">One\
                    <a class=\"anchor\" href=\"#one\" aria-hidden=\"true\" tabindex=\"-1\">#</a>\
                    </h2><p>Some prose.</p></main>";
        assert_eq!(text(html), "One Some prose.");
        assert_eq!(
            Text::prose(html, Region::default()),
            "<h2 id=\"one\">One</h2><p>Some prose.</p>"
        );
        // `aria-hidden="false"` is the author saying the opposite.
        let shown = "<main><span aria-hidden=\"false\">kept</span></main>";
        assert_eq!(text(shown), "kept");
    }

    /// A script is neither prose nor markup a reader runs, and an element that
    /// never closes is left alone rather than swallowing the rest of the page.
    #[test]
    fn a_feed_body_drops_scripts_and_keeps_an_unclosed_element() {
        let html = "<main><p>a</p><script>go()</script><br><p>b</p></main>";
        let ignore = ["br".to_owned()];
        let body = Text::prose(
            html,
            Region {
                element: "main",
                ignore: &ignore,
            },
        );
        assert_eq!(body, "<p>a</p><br><p>b</p>");
    }
}
