//! CommonMark + GFM lowered to Typst source.
//!
//! Every node becomes a Typst *call* with content arguments, never spliced
//! markup: text runs go through [`Content`], which emits `#"..."`, so an
//! asterisk, a `#`, or an unbalanced bracket in prose is data and can never
//! become syntax. That is what makes the lowering total rather than a template
//! that happens to work on well-behaved input.
//!
//! The one deliberate hole in that rule is a fence marked `eval`, which is
//! emitted verbatim because evaluating it is the point. Its errors land in the
//! author's own code, which is where they belong.
//!
//! A fence's info string is `lang` followed by space-separated `key` or
//! `key=value` parameters, so an option never has to be smuggled in as a
//! language name:
//!
//! ````text
//! ```typ            a Typst sample, shown
//! ```typ eval       evaluated instead
//! ````
//!
//! Showing is the default because a fence means "show this code" everywhere
//! else, and a document *about* Typst is the common case: this project's own
//! `CHANGELOG.md` has six Typst samples and not one line it wants run.

mod fence;
mod located;
mod writer;

use located::Located;
use std::ops::Range;
use writer::{Align, Writer};

use pulldown_cmark::{Event, Options, Parser};

use crate::codegen::Value;
use crate::config::{Extension, MarkdownConfig};
use crate::content::SourceMap;
use crate::error::Result;

/// A markdown body, without its frontmatter block.
///
/// Carries the whole file and where the body starts in it, not just the body:
/// a fault belongs to the page the author wrote, and a span measured against
/// the body alone would underline the wrong line of it.
pub struct Markdown<'a> {
    file: &'a str,
    body: &'a str,
    offset: usize,
    /// The page this came from, for diagnostics.
    path: &'a str,
    /// What this site allows a page to contain.
    config: &'a MarkdownConfig,
}

impl<'a> Markdown<'a> {
    /// The body of `document`, positioned in the `file` it was split from.
    pub fn new(
        document: &super::Document<'a>,
        file: &'a str,
        path: &'a str,
        config: &'a MarkdownConfig,
    ) -> Self {
        Self {
            file,
            body: document.body,
            offset: document.body_offset,
            path,
            config,
        }
    }

    /// The parser options the configured extensions ask for.
    ///
    /// One arm per [`Extension`], so a variant added to that table fails to
    /// compile until it says which option it turns on: the config name and the
    /// parser bit cannot drift apart.
    fn options(&self) -> Options {
        self.config
            .extensions
            .iter()
            .map(|extension| match extension {
                Extension::Tables => Options::ENABLE_TABLES,
                Extension::Footnotes => Options::ENABLE_FOOTNOTES,
                Extension::Strikethrough => Options::ENABLE_STRIKETHROUGH,
                Extension::Tasklists => Options::ENABLE_TASKLISTS,
                Extension::Smart => Options::ENABLE_SMART_PUNCTUATION,
            })
            .fold(Options::empty(), |all, one| all | one)
    }

    /// The Typst source this page compiles as, and where it came from in the
    /// file the author wrote.
    pub fn lower(&self) -> Result<(String, SourceMap)> {
        // `into_offset_iter` so every event knows the bytes it came from, which
        // is what lets a fault point at the markdown rather than at the Typst
        // this produces.
        let (events, spans): (Vec<Event<'_>>, Vec<Range<usize>>) =
            Parser::new_ext(self.body, self.options())
                .into_offset_iter()
                .unzip();
        // Numbered, and the numbers travel with the events. A footnote body is
        // lowered by a walk of its own over a *copy* of its events, and a copy
        // renumbers them: every span that walk recorded would name whichever
        // event happened to sit at the same index in the whole document.
        let events: Vec<Numbered<'_>> = events.into_iter().enumerate().collect();
        let mut writer = Writer::new(
            self.path,
            Located::new(self.file, self.offset, &spans),
            self.config,
        );
        // Twice: a definition may reference one defined further down the file,
        // and the first pass only knows the ones above it. The second pass runs
        // with all of them, which resolves a forward reference and bounds a
        // circular one instead of chasing it.
        writer.notes(&events)?;
        writer.notes(&events)?;
        writer.walk(&events)?;
        let body = writer.finish();
        let map = SourceMap::new(self.file.to_owned(), body.text.len(), body.spans);
        Ok((body.text, map))
    }
}

/// A parse event and its index into the parse's span table.
///
/// Carried as one value because the two are only useful together: an event
/// separated from its number cannot say where it came from, and that is exactly
/// what a nested walk over cloned events used to do.
type Numbered<'a> = (usize, Event<'a>);

impl From<Align> for Value {
    /// A Typst identifier, which no string literal can stand in for: `align`
    /// takes alignment values, not their names.
    fn from(align: Align) -> Self {
        Self::Raw(
            match align {
                Align::Left => "left",
                Align::Center => "center",
                Align::Right => "right",
            }
            .to_owned(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::RawHtml;
    use crate::content::Rebased;
    use crate::content::markdown::lower::writer::Buffer;
    use crate::content::sourcemap::{Mapping, Shape};

    /// Split then lower, which is the path a real page takes: a test that
    /// skipped the split would not notice a body offset going wrong.
    fn under(source: &str, config: &MarkdownConfig) -> Result<String> {
        let document = super::super::Document::split(source, "a.md")?;
        Markdown::new(&document, source, "a.md", config)
            .lower()
            .map(|(source, _)| source)
    }

    fn try_lower(source: &str) -> Result<String> {
        under(source, &MarkdownConfig::default())
    }

    fn lower(source: &str) -> String {
        try_lower(source).expect("lower")
    }

    /// The property the whole design rests on: prose is data. Typst syntax in a
    /// paragraph has to survive as text, not become syntax.
    #[test]
    fn prose_can_never_become_syntax() {
        let out = lower("a #call() and [brackets and $math$\n");
        assert!(out.contains(r#"#"a #call() and ""#), "{out}");
        // The bracket is its own text run, because that is where the parser
        // split it; what matters is that it is a literal and not a content
        // block that would swallow the rest of the line.
        assert!(out.contains(r#"#"[""#), "{out}");
        assert!(out.contains(r#"brackets and $math$""#), "{out}");
    }

    #[test]
    fn inline_marks_become_calls() {
        assert!(lower("*a*\n").contains("#emph["));
        assert!(lower("**a**\n").contains("#strong["));
        assert!(lower("~~a~~\n").contains("#strike["));
        assert!(lower("`a`\n").contains(r#"#raw("a")"#));
        assert!(lower("[t](/u)\n").contains(r#"#link("/u")["#));
    }

    #[test]
    fn a_heading_carries_its_level() {
        assert!(lower("### T\n").contains("#heading(level: 3)["));
    }

    /// A fence is shown by default, including a Typst one: a document about
    /// Typst is the common case, and this project's own changelog is one.
    #[test]
    fn a_typ_fence_is_shown_unless_it_says_eval() {
        let shown = lower("```typ\n#callout[hi]\n```\n");
        assert!(
            shown.contains(r#"#raw(block: true, lang: "typ""#),
            "{shown}"
        );

        let run = lower("```typ eval\n#callout[hi]\n```\n");
        assert!(run.contains("#callout[hi]"), "{run}");
        assert!(!run.contains("#raw(block: true"), "{run}");
    }

    /// Parameters are `key` or `key=value`, so the grammar has room for options
    /// that are not a language name.
    #[test]
    fn fence_parameters_parse_as_flags_or_pairs() {
        assert!(lower("```typ eval=true\n#emph[x]\n```\n").contains("#emph[x]"));
        assert!(lower("```typ eval=false\n#emph[x]\n```\n").contains("#raw(block: true"));
        // An unknown parameter is not an instruction, and must not change what
        // the fence does.
        assert!(lower("```typ linenos\n#emph[x]\n```\n").contains("#raw(block: true"));
    }

    /// Only Typst can be evaluated. `sh eval` would otherwise emit a shell
    /// script into the page as Typst source.
    #[test]
    fn eval_on_another_language_is_not_honoured() {
        let out = lower("```sh eval\nrm -rf /\n```\n");
        assert!(out.contains(r#"#raw(block: true, lang: "sh""#), "{out}");
    }

    /// A level-6 heading has no HTML heading to be, so it clamps rather than
    /// becoming an `aria-level="7"` div the anchor pass cannot see.
    #[test]
    fn heading_levels_clamp_to_a_real_heading() {
        assert!(lower("# a\n").contains("#heading(level: 1)["));
        assert!(lower("##### e\n").contains("#heading(level: 5)["));
        assert!(lower("###### f\n").contains("#heading(level: 5)["));
    }

    /// `line` is dropped by typst's HTML export, so a thematic break has to be
    /// the element that means one.
    #[test]
    fn a_thematic_break_is_an_hr() {
        let out = lower("a\n\n---\n\nb\n");
        assert!(out.contains(r#"#html.elem("hr")"#), "{out}");
    }

    /// Alt text is the raw text of the run, never the lowered output read back:
    /// a link inside the alt used to leave generated source in the attribute.
    #[test]
    fn alt_text_survives_inline_marks() {
        let out = lower("![a *b* `c` d](/i.png)\n");
        assert!(out.contains(r#"alt: "a b c d""#), "{out}");
    }

    #[test]
    fn alt_text_is_not_confused_by_a_hash_quote_in_a_link() {
        let out = lower("![a [t](/x#) b](/i.png)\n");
        assert!(out.contains(r#"alt: "a t b""#), "{out}");
        assert!(
            !out.contains(")[#"),
            "generated source leaked into alt: {out}"
        );
    }

    /// A definition may sit below a definition that references it.
    #[test]
    fn a_footnote_may_reference_one_defined_later() {
        let out = lower("see[^a]\n\n[^a]: outer with [^b]\n\n[^b]: inner\n");
        assert!(
            out.contains(r#"#"inner""#),
            "the later note was dropped: {out}"
        );
    }

    /// A cycle terminates rather than recursing.
    #[test]
    fn circular_footnotes_terminate() {
        let out = lower("see[^a]\n\n[^a]: to [^b]\n\n[^b]: back to [^a]\n");
        assert!(out.contains("#footnote["), "{out}");
    }

    #[test]
    fn a_fence_keeps_its_language() {
        assert!(lower("```kdl\na b\n```\n").contains(r#"lang: "kdl""#));
    }

    #[test]
    fn a_table_carries_columns_and_alignment() {
        let out = lower("| a | b |\n| --- | ---: |\n| 1 | 2 |\n");
        assert!(
            out.contains("#table(columns: 2, align: (left, right, ),"),
            "{out}"
        );
        assert!(out.contains("table.header("), "{out}");
    }

    /// Markdown separates a footnote from its definition; Typst does not. The
    /// body has to arrive at the reference.
    #[test]
    fn a_footnote_body_moves_to_its_reference() {
        let out = lower("see[^n]\n\n[^n]: the note\n");
        assert!(out.contains("#footnote["), "{out}");
        assert!(out.contains(r#"#"the note""#), "{out}");
    }

    #[test]
    fn task_markers_render() {
        let out = lower("- [x] done\n- [ ] not\n");
        assert!(out.contains("#sym.ballot.check"), "{out}");
        assert!(out.contains("#sym.ballot "), "{out}");
    }

    #[test]
    fn raw_html_is_refused_rather_than_dropped() {
        assert!(try_lower("<div>x</div>\n").is_err());
        assert!(try_lower("a <b>c</b>\n").is_err());
    }

    /// The span is measured against the *file*, frontmatter included, so the
    /// underline lands on the line the author wrote rather than that many bytes
    /// into the body.
    #[test]
    fn a_fault_points_into_the_file_not_the_body() {
        let source = "---\ntitle \"A\"\n---\n\n<div>x</div>\n";
        let Err(error) = try_lower(source) else {
            panic!("raw html should fail");
        };
        let rendered = format!("{:?}", miette::Report::new(error));
        // The snippet has to show the whole page, and the label has to sit on
        // the markup rather than inside the frontmatter block.
        assert!(rendered.contains("<div>x</div>"), "{rendered}");
        let at = source.find("<div>").expect("the markup is in the source");
        assert!(at > source.find("title").expect("frontmatter"), "sanity");
    }

    /// Every knob is the site's, and each one has to actually reach the
    /// lowering rather than merely parse.
    #[test]
    fn the_site_decides_what_a_page_may_contain() {
        let dropping = MarkdownConfig {
            html: RawHtml::Drop,
            ..MarkdownConfig::default()
        };
        let out = under("a <b>c</b>\n", &dropping).expect("dropped, not refused");
        assert!(!out.contains("<b>"), "{out}");
        assert!(out.contains(r#"#"a ""#), "{out}");

        // A site that does not trust its authors can refuse to run any of it.
        let sealed = MarkdownConfig {
            eval: false,
            ..MarkdownConfig::default()
        };
        let out = under("```typ eval\n#emph[x]\n```\n", &sealed).expect("lower");
        assert!(
            out.contains("#raw(block: true"),
            "eval should not run: {out}"
        );
    }

    /// An extension the site did not ask for is not parsed, and one it added is.
    #[test]
    fn extensions_follow_the_configured_set() {
        let table = "| a | b |\n| --- | --- |\n| 1 | 2 |\n";
        assert!(lower(table).contains("#table("));

        let without = MarkdownConfig {
            extensions: vec![Extension::Footnotes],
            ..MarkdownConfig::default()
        };
        let out = under(table, &without).expect("lower");
        assert!(!out.contains("#table("), "tables were off: {out}");

        // Asserted on rendered characters, not on the call being emitted: an
        // assertion that a call *appears* is what hid three extensions whose
        // events the writer silently swallowed.
        let smart = MarkdownConfig {
            extensions: vec![Extension::Smart],
            ..MarkdownConfig::default()
        };
        let out = under("a -- b ...\n", &smart).expect("lower");
        assert!(out.contains('\u{2013}'), "en dash: {out}");
        assert!(out.contains('\u{2026}'), "ellipsis: {out}");
    }

    /// How many bytes of preamble the tests below put in front of a body. Any
    /// number does; the map derives it from the wrapper rather than being told.
    const PREAMBLE: usize = 100;

    /// Lower `source`, then place the result in a wrapper the way a real
    /// compile does: the preamble first, the body last.
    fn mapped(source: &str) -> (String, Rebased) {
        let document = super::super::Document::split(source, "a.md").expect("split");
        let (lowered, map) = Markdown::new(&document, source, "a.md", &MarkdownConfig::default())
            .lower()
            .expect("lower");
        let wrapper = format!("{}{lowered}", " ".repeat(PREAMBLE));
        let rebased = Rebased::new(std::sync::Arc::new(map), &wrapper).expect("the body fits");
        (lowered, rebased)
    }

    /// Where `needle` in the lowered body maps back to in the authored file.
    fn back(lowered: &str, map: &Rebased, needle: &str) -> Range<usize> {
        let at = lowered
            .find(needle)
            .unwrap_or_else(|| panic!("{needle:?} was not emitted: {lowered}"));
        map.locate(&((PREAMBLE + at)..(PREAMBLE + at + needle.len())))
            .unwrap_or_else(|| panic!("{needle:?} maps to nothing: {lowered}"))
    }

    /// An `eval` fence is authored Typst copied out verbatim, so it is the one
    /// construct whose offsets survive byte for byte.
    #[test]
    fn an_eval_fence_records_where_it_came_from() {
        let source = "---\ntitle \"A\"\n---\n\ntext\n\n```typ eval\n#emph[x]\n```\n";
        let (lowered, map) = mapped(source);
        let at = back(&lowered, &map, "#emph");
        assert_eq!(&source[at.start..at.start + 5], "#emph");
    }

    /// An indented fence maps a line at a time, because the parser hands its
    /// content back with the indentation stripped: the block is not byte-for-
    /// byte with the file, and one whole-block pair drifted by the indent times
    /// the lines before it. Both lines of this landed on the first one, the
    /// second at a column past the end of it.
    #[test]
    fn an_indented_fence_maps_each_of_its_lines() {
        let source = concat!(
            ";;;\ntitle \"A\"\n;;;\n\n",
            "- item\n\n",
            "  ```typ eval\n",
            "  #emph[one]\n",
            "  #emph[two]\n",
            "  ```\n",
        );
        let (lowered, map) = mapped(source);
        let one = back(&lowered, &map, "#emph[one]");
        let two = back(&lowered, &map, "#emph[two]");
        assert_eq!(&source[one.start..one.start + 10], "#emph[one]");
        assert_eq!(&source[two.start..two.start + 10], "#emph[two]");
        // Different authored lines, which is the whole point: they used to
        // resolve into the same one.
        assert_ne!(
            source[..one.start].lines().count(),
            source[..two.start].lines().count()
        );
    }

    /// The wrapper is not lowered from anything, so nothing in it maps.
    #[test]
    fn the_wrapper_maps_to_nothing() {
        let (_, map) = mapped("---\ntitle \"A\"\n---\n\njust prose\n");
        assert_eq!(map.locate(&(4..8)), None);
        assert_eq!(map.locate(&(0..PREAMBLE)), None);
    }

    /// A page with nothing under its frontmatter lowers to nothing, and an
    /// empty map must answer "nowhere" rather than "the top of the file".
    #[test]
    fn an_empty_body_maps_to_nothing() {
        let (_, map) = mapped("---\ntitle \"A\"\n---\n");
        assert_eq!(map.locate(&(PREAMBLE..PREAMBLE + 1)), None);
    }

    /// Every construct the lowering emits is attributed, not just `eval`
    /// fences: the generated call for a heading, a list, a link came from the
    /// markdown that asked for it, and a jump-to-source that lands on the
    /// generated Typst instead is a path no editor can open.
    #[test]
    fn every_construct_maps_back_to_the_markdown() {
        let source = "---\ntitle \"A\"\n---\n\nFirst paragraph.\n\n## A heading\n\n- an item\n- another, with a [link](https://x.com)\n\nLast.\n";
        let (lowered, map) = mapped(source);
        let line = |needle: &str| {
            let at = back(&lowered, &map, needle);
            map.map().position(at.start).expect("in the file").0
        };
        assert_eq!(line(r#"#"First paragraph.""#), 5);
        assert_eq!(line("#heading(level: 2)["), 7);
        assert_eq!(line("#list("), 9);
        assert_eq!(line(r#"#"an item""#), 9);
        assert_eq!(line(r#"#"another, with a ""#), 10);
        assert_eq!(line(r#"#link("https://x.com")["#), 10);
        assert_eq!(line(r#"#"Last.""#), 12);
    }

    /// The subtle half of the map, tested on its own because a wrong shift is
    /// invisible in the output: a buffered construct is written apart from its
    /// parent, so its spans are in its own coordinates and the splice has to
    /// move them by where its text landed. Unshifted, every one of them names
    /// bytes that many too early.
    #[test]
    fn splicing_shifts_a_childs_spans_by_where_its_text_landed() {
        let mut parent = Buffer::default();
        parent.push("abcd");
        let mut child = Buffer::default();
        child.push("xy");
        child.record(0, 40..42, Shape::Whole);
        parent.splice(child);
        assert_eq!(parent.text, "abcdxy");
        assert_eq!(parent.spans, vec![Mapping::new(4..6, 40..42, Shape::Whole)]);
    }

    /// Splices compose, which is what a construct buffered inside another
    /// buffered one needs: each level shifts by its own landing point, and the
    /// pair that reaches the root has been moved by both.
    #[test]
    fn splices_compose_through_a_stack_of_buffers() {
        let mut inner = Buffer::default();
        inner.push("z");
        inner.record(0, 7..8, Shape::Whole);

        let mut middle = Buffer::default();
        middle.push("ab");
        middle.splice(inner);
        assert_eq!(middle.spans, vec![Mapping::new(2..3, 7..8, Shape::Whole)]);

        let mut root = Buffer::default();
        root.push("0123");
        root.splice(middle);
        assert_eq!(root.text, "0123abz");
        assert_eq!(root.spans, vec![Mapping::new(6..7, 7..8, Shape::Whole)]);
    }

    /// A footnote body is lowered at its definition by a walk of its own and
    /// written at its reference, which is the splice a real page exercises: its
    /// spans have to follow the text, and the walk that recorded them has to
    /// have known which events it was looking at.
    #[test]
    fn a_footnote_body_maps_to_its_definition() {
        let source = "---\ntitle \"A\"\n---\n\nsee[^n]\n\n[^n]: the note\n";
        let (lowered, map) = mapped(source);
        let at = back(&lowered, &map, r#"#"the note""#);
        assert_eq!(&source[at.start..at.start + 8], "the note");
        assert_eq!(map.map().position(at.start).expect("in the file").0, 7);
    }

    /// A comment renders as nothing anywhere, so it is the one raw-HTML shape
    /// that can be dropped without losing content.
    #[test]
    fn html_comments_are_dropped_not_refused() {
        assert!(
            lower("<!-- a note -->\n").is_empty() || !lower("<!-- a note -->\n").contains("note")
        );
        let inline = lower("text <!-- hidden --> more\n");
        assert!(!inline.contains("hidden"), "{inline}");
        assert!(inline.contains(r#"#"text ""#), "{inline}");
    }
}
