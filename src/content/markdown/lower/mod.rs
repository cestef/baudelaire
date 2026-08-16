//! CommonMark + GFM lowered to Typst source.
//!
//! Every node becomes a Typst *call* with content arguments, never spliced
//! markup, so prose can never become syntax; a fence marked `eval` is the one
//! exception and is emitted verbatim. A fence's info string is `lang` followed
//! by space-separated `key` or `key=value` parameters:
//!
//! ````text
//! ```typ            a Typst sample, shown
//! ```typ eval       evaluated instead
//! ````

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

/// A markdown body, without its frontmatter block; it carries the whole file
/// and where the body starts in it, so a fault underlines the line the author
/// wrote rather than that many bytes into the body.
pub struct Markdown<'a> {
    file: &'a str,
    body: &'a str,
    offset: usize,
    path: &'a str,
    config: &'a MarkdownConfig,
}

impl<'a> Markdown<'a> {
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
    /// file the author wrote; the note pass runs twice so a definition may
    /// reference one defined further down the file.
    pub fn lower(&self) -> Result<(String, SourceMap)> {
        let (events, spans): (Vec<Event<'_>>, Vec<Range<usize>>) =
            Parser::new_ext(self.body, self.options())
                .into_offset_iter()
                .unzip();
        let events: Vec<Numbered<'_>> = events.into_iter().enumerate().collect();
        let mut writer = Writer::new(
            self.path,
            Located::new(self.file, self.offset, &spans),
            self.config,
        );
        writer.notes(&events)?;
        writer.notes(&events)?;
        writer.walk(&events)?;
        let body = writer.finish();
        let map = SourceMap::new(self.file.to_owned(), body.text.len(), body.spans);
        Ok((body.text, map))
    }
}

/// A parse event and its index into the parse's span table, carried together so
/// a walk over cloned events can still say where each came from.
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

    /// Split then lower, which is the path a real page takes.
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

    #[test]
    fn prose_can_never_become_syntax() {
        let out = lower("a #call() and [brackets and $math$\n");
        assert!(out.contains(r#"#"a #call() and ""#), "{out}");
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

    #[test]
    fn fence_parameters_parse_as_flags_or_pairs() {
        assert!(lower("```typ eval=true\n#emph[x]\n```\n").contains("#emph[x]"));
        assert!(lower("```typ eval=false\n#emph[x]\n```\n").contains("#raw(block: true"));
        assert!(lower("```typ linenos\n#emph[x]\n```\n").contains("#raw(block: true"));
    }

    #[test]
    fn eval_on_another_language_is_not_honoured() {
        let out = lower("```sh eval\nrm -rf /\n```\n");
        assert!(out.contains(r#"#raw(block: true, lang: "sh""#), "{out}");
    }

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

    #[test]
    fn a_footnote_may_reference_one_defined_later() {
        let out = lower("see[^a]\n\n[^a]: outer with [^b]\n\n[^b]: inner\n");
        assert!(
            out.contains(r#"#"inner""#),
            "the later note was dropped: {out}"
        );
    }

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

    #[test]
    fn a_fault_points_into_the_file_not_the_body() {
        let source = "---\ntitle \"A\"\n---\n\n<div>x</div>\n";
        let Err(error) = try_lower(source) else {
            panic!("raw html should fail");
        };
        let rendered = format!("{:?}", miette::Report::new(error));
        assert!(rendered.contains("<div>x</div>"), "{rendered}");
        let at = source.find("<div>").expect("the markup is in the source");
        assert!(at > source.find("title").expect("frontmatter"), "sanity");
    }

    #[test]
    fn the_site_decides_what_a_page_may_contain() {
        let dropping = MarkdownConfig {
            html: RawHtml::Drop,
            ..MarkdownConfig::default()
        };
        let out = under("a <b>c</b>\n", &dropping).expect("dropped, not refused");
        assert!(!out.contains("<b>"), "{out}");
        assert!(out.contains(r#"#"a ""#), "{out}");

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

        let smart = MarkdownConfig {
            extensions: vec![Extension::Smart],
            ..MarkdownConfig::default()
        };
        let out = under("a -- b ...\n", &smart).expect("lower");
        assert!(out.contains('\u{2013}'), "en dash: {out}");
        assert!(out.contains('\u{2026}'), "ellipsis: {out}");
    }

    /// How many bytes of preamble the tests below put in front of a body.
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

    #[test]
    fn an_eval_fence_records_where_it_came_from() {
        let source = "---\ntitle \"A\"\n---\n\ntext\n\n```typ eval\n#emph[x]\n```\n";
        let (lowered, map) = mapped(source);
        let at = back(&lowered, &map, "#emph");
        assert_eq!(&source[at.start..at.start + 5], "#emph");
    }

    /// An indented fence maps a line at a time, because the parser hands its
    /// content back with the indentation stripped.
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
        assert_ne!(
            source[..one.start].lines().count(),
            source[..two.start].lines().count()
        );
    }

    #[test]
    fn the_wrapper_maps_to_nothing() {
        let (_, map) = mapped("---\ntitle \"A\"\n---\n\njust prose\n");
        assert_eq!(map.locate(&(4..8)), None);
        assert_eq!(map.locate(&(0..PREAMBLE)), None);
    }

    #[test]
    fn an_empty_body_maps_to_nothing() {
        let (_, map) = mapped("---\ntitle \"A\"\n---\n");
        assert_eq!(map.locate(&(PREAMBLE..PREAMBLE + 1)), None);
    }

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

    /// A buffered construct is written apart from its parent, so its spans are
    /// in its own coordinates and the splice moves them by where its text
    /// landed.
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
    ///
    /// `<!-->` and `<!--->` are empty comments, which must not be refused.
    #[test]
    fn html_comments_are_dropped_not_refused() {
        assert!(
            lower("<!-- a note -->\n").is_empty() || !lower("<!-- a note -->\n").contains("note")
        );
        let inline = lower("text <!-- hidden --> more\n");
        assert!(!inline.contains("hidden"), "{inline}");
        assert!(inline.contains(r#"#"text ""#), "{inline}");
        assert!(try_lower("<!-->\n").is_ok());
        assert!(try_lower("<!--->\n").is_ok());
    }

    /// CommonMark ends an HTML block on the line carrying `-->`, so a run that
    /// opens and closes like a comment may hide an element between.
    #[test]
    fn a_comment_cannot_smuggle_markup_past_the_html_policy() {
        let source = "<!-- a --><div>SECRET</div><!-- b -->\n";
        assert!(try_lower(source).is_err(), "{source}");
        assert!(try_lower("<!--><div>SECRET</div>-->\n").is_err());

        let dropping = MarkdownConfig {
            html: RawHtml::Drop,
            ..MarkdownConfig::default()
        };
        let out = under(source, &dropping).expect("dropped, not refused");
        assert!(!out.contains("SECRET"), "{out}");
    }

    /// An email autolink's destination is the bare address, which without the
    /// scheme is a relative path to a page nobody has.
    #[test]
    fn an_email_autolink_keeps_its_scheme() {
        let out = lower("<me@example.com>\n");
        assert!(out.contains(r#"#link("mailto:me@example.com")["#), "{out}");
        let written = lower("[x](mailto:me@example.com)\n");
        assert!(
            written.contains(r#"#link("mailto:me@example.com")["#),
            "{written}"
        );
        assert!(lower("<https://x.com>\n").contains(r#"#link("https://x.com")["#));
    }

    /// A break inside an alt run is the space between the two lines it
    /// separated.
    #[test]
    fn a_multi_line_alt_keeps_the_space_between_its_lines() {
        let soft = lower("![alpha\nbeta](/i.png)\n");
        assert!(soft.contains(r#"alt: "alpha beta""#), "{soft}");
        let hard = lower("![alpha  \nbeta](/i.png)\n");
        assert!(hard.contains(r#"alt: "alpha beta""#), "{hard}");
        assert!(lower("a\nb\n").contains(r#"#" ""#));
        assert!(lower("a  \nb\n").contains("#linebreak()"));
    }

    /// Math is not an extension a site can enable, so a dollar run is prose and
    /// stays prose.
    #[test]
    fn math_is_never_parsed_so_a_dollar_run_is_text() {
        let out = lower("an $x^2$ run\n");
        assert!(out.contains("$x^2$"), "{out}");
        assert!(!out.contains("math.equation"), "{out}");
    }
}
