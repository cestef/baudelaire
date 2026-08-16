//! Plain-text extraction from rendered HTML, and the reading estimate taken
//! from a page's typst source before it is rendered at all. A tag-aware
//! scanner, not an HTML parser, scoped to one region of the page ([`Region`])
//! so site chrome never pollutes the prose.

/// The predefined HTML/XML entities as `(char, name)`, read when decoding
/// `&name;` back to its character during extraction.
const ENTITIES: &[(char, &str)] = &[
    ('<', "lt"),
    ('>', "gt"),
    ('"', "quot"),
    ('\'', "apos"),
    ('&', "amp"),
];

/// Which part of a page is prose: the element its text is taken from, and the
/// elements inside it that are not text at all, both as tag names.
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
            element: crate::config::RegionConfig::MAIN,
            ignore: &[],
        }
    }
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

    /// The inner HTML of the first `element`, or the whole document when there
    /// is none, so a page without the region is read whole rather than not at
    /// all. An empty `element` is every element there is.
    pub fn region<'a>(html: &'a str, element: &str) -> &'a str {
        if element.is_empty() {
            return html;
        }
        let Some(open) = Self::opening(html, 0, element) else {
            return html;
        };
        let Some(gt) = html[open..].find('>') else {
            return html;
        };
        let start = open + gt + 1;
        match Self::closing(html, start, element) {
            Some(end) => &html[start..end],
            None => &html[start..],
        }
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
    /// whitespace in a single forward pass, skipping an `ignore`d element whole
    /// the way a `<script>` is. Content runs are sliced at ASCII markers only,
    /// which keeps every slice on a UTF-8 boundary.
    fn scan(html: &str, ignore: &[String]) -> String {
        let bytes = html.as_bytes();
        let mut out = String::with_capacity(html.len() / 2);
        let mut gap = false;
        let mut i = 0;
        while i < bytes.len() {
            match bytes[i] {
                b'<' => {
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
    /// one marked `aria-hidden="true"`, which keeps a heading's own self link
    /// out of the prose.
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

/// How long a page takes to read, as the prose words it carries, counted from
/// the page's *source*: the render has not happened when a template is handed
/// its page. Words and not minutes, because the rate is the site's and this is
/// measured before a page knows its language; [`Reading::minutes`] applies it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Reading {
    pub words: usize,
}

impl Reading {
    /// Estimate `body`, a page's typst source. A leading `#` starts code, so
    /// such a line is machinery; everything else is prose, markup and all.
    pub fn of(body: &str) -> Self {
        Self::counted(
            body.lines()
                .map(str::trim_start)
                .filter(|line| !line.starts_with('#') && !line.starts_with("//"))
                .map(Self::words)
                .sum(),
        )
    }

    /// Estimate `body`, the markdown an author wrote: it has to be the authored
    /// text, since every line the lowering emits begins with `#` and would read
    /// as machinery. A fenced code block is what a `#`-line is in typst, and a
    /// heading is prose here.
    #[cfg(feature = "markdown")]
    pub fn markdown(body: &str) -> Self {
        let mut fence: Option<&str> = None;
        let mut words = 0;
        for line in body.lines() {
            let line = line.trim_start();
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
    /// backticks or tildes, without whatever info string follows it. Both
    /// markers are one byte, so the run's length in bytes is its length in
    /// characters.
    #[cfg(feature = "markdown")]
    fn opens(line: &str) -> Option<&str> {
        let marker = line.chars().next().filter(|c| *c == '`' || *c == '~')?;
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
    pub fn minutes(self, wpm: usize) -> usize {
        self.words.div_ceil(wpm.max(1))
    }

    fn counted(words: usize) -> Self {
        Self { words }
    }
}

#[cfg(test)]
mod tests {
    use super::{Region, Text};

    /// The default region, for the cases that are about the scanner.
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
        assert_eq!(text(html), "Title real content");
    }

    #[test]
    fn the_region_is_whatever_the_site_names() {
        let html = "<nav>chrome</nav><article><p>prose</p></article>";
        let article = Region {
            element: "article",
            ..Region::default()
        };
        assert_eq!(Text::extract(html, article), "prose");
        assert_eq!(text(html), "chrome prose");
        let all = Region {
            element: "",
            ..Region::default()
        };
        assert_eq!(Text::extract(html, all), "chrome prose");
    }

    #[test]
    fn a_region_matches_a_whole_tag_name() {
        let html = "<main-menu>chrome</main-menu><main>prose</main>";
        assert_eq!(text(html), "prose");
    }

    #[test]
    fn a_nested_region_does_not_end_the_outer_one() {
        let html = "<main>before <main>inner</main> after</main><footer>chrome</footer>";
        assert_eq!(text(html), "before inner after");
    }

    #[test]
    fn ignored_elements_are_dropped_from_the_region() {
        let html = "<main><nav>Home About</nav><p>prose</p><aside>related</aside></main>";
        let region = Region {
            element: crate::config::RegionConfig::MAIN,
            ignore: &["nav".to_owned(), "aside".to_owned()],
        };
        assert_eq!(Text::extract(html, region), "prose");
        let nested = "<main><nav>a<nav>b</nav>c</nav><p>prose</p></main>";
        assert_eq!(Text::extract(nested, region), "prose");
    }

    #[test]
    fn reading_counts_prose_and_skips_typst_code() {
        use super::Reading;
        let body = "#import \"/templates/theme.typ\": callout\n\
                    #let x = 1\n\
                    // a comment\n\
                    \n\
                    = A heading\n\
                    Three plain words, plus #emph[one] more.\n";
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
        assert_eq!(Reading::markdown(body).words, 11);
        assert_eq!(Reading::markdown(body).minutes(200), 1);
        let lowered = "#heading(level: 2)[#\"A heading\"]\n\
                       #\"Three plain words, plus \"#strong[#\"one\"]#\" more.\"\n";
        assert_eq!(Reading::of(lowered).words, 0);
        assert_eq!(Reading::markdown("").words, 0);
    }

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
        assert_eq!(Reading::markdown("`` a b ``\n").words, 2);
    }

    #[test]
    fn what_a_reader_is_told_to_ignore_is_not_indexed() {
        let html = "<main><h2 id=\"one\">One\
                    <a class=\"anchor\" href=\"#one\" aria-hidden=\"true\" tabindex=\"-1\">#</a>\
                    </h2><p>Some prose.</p></main>";
        assert_eq!(text(html), "One Some prose.");
        let shown = "<main><span aria-hidden=\"false\">kept</span></main>";
        assert_eq!(text(shown), "kept");
    }
}
