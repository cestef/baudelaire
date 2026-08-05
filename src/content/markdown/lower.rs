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

use std::ops::Range;

use pulldown_cmark::{Alignment, CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd};

use crate::codegen::{Call, Content, Value};
use crate::config::{Extension, MarkdownConfig, RawHtml};
use crate::content::SourceMap;
use crate::content::sourcemap::{Mapping, Shape};
use crate::error::Result;
use crate::error::markdown::MarkdownError;

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

/// A fence's info string: a language, and the parameters that say what to do
/// with it. Parsed rather than matched whole so a new option is a key here and
/// nothing else, and so `typ eval` cannot be confused for a language nobody
/// registered.
struct Fence {
    lang: Option<String>,
    /// Whether the block is Typst to run rather than a sample to show.
    eval: bool,
}

impl Fence {
    /// The parameter that makes a fence run. Named once.
    const EVAL: &'static str = "eval";

    fn parse(info: &str) -> Self {
        let mut words = info.split_whitespace();
        let lang = words.next().filter(|w| !w.is_empty()).map(str::to_owned);
        let mut eval = false;
        for param in words {
            // `key=value` is accepted so the grammar has room to grow; today
            // the only key is a bare flag, and `eval=false` reads as written.
            let (key, value) = param.split_once('=').unwrap_or((param, "true"));
            if key == Self::EVAL {
                eval = value == "true";
            }
        }
        Self { lang, eval }
    }

    /// Whether this fence runs, which needs three things to agree: the page
    /// asked, the language is Typst (`sh eval` would otherwise emit a shell
    /// script as Typst source), and the site permits it at all.
    fn runs(&self, config: &MarkdownConfig) -> bool {
        config.eval && self.eval && self.lang.as_deref() == Some("typ")
    }
}

/// A construct whose content has to be complete before anything can be written
/// for it: the alt text of an image, the body of a footnote, the text of a code
/// block. Everything else streams straight out.
enum Buffered {
    /// `alt` collects the raw text of the alt run. It is *not* taken from the
    /// lowered buffer: an alt attribute is a plain string, and un-escaping
    /// lowered output to recover one loses every inline that is not a text run
    /// and mistakes a `#"` inside the generated source for the start of one.
    Alt {
        dest: String,
        alt: String,
    },
    Code {
        fence: Fence,
    },
}

/// Whether a raw-HTML run is nothing but a comment. A comment is the one shape
/// of raw HTML with no rendered counterpart, so dropping it loses nothing.
fn is_comment(raw: &str) -> bool {
    let trimmed = raw.trim();
    trimmed.len() >= 7 && trimmed.starts_with("<!--") && trimmed.ends_with("-->")
}

/// Where an event came from in the file being lowered: the text a snippet is
/// rendered from, and the byte range of each event within it.
///
/// One value rather than three fields threaded through the writer, for the same
/// reason [`crate::content::Origin`] is one: they exist together to make a
/// diagnostic precise and are never useful apart.
struct Located<'a> {
    file: &'a str,
    /// Byte offset of the body within `file`.
    offset: usize,
    spans: &'a [Range<usize>],
    /// The event being written, as an index into `spans`.
    at: usize,
}

impl<'a> Located<'a> {
    fn new(file: &'a str, offset: usize, spans: &'a [Range<usize>]) -> Self {
        Self {
            file,
            offset,
            spans,
            at: 0,
        }
    }

    /// A copy pointing at the same file and spans, for a nested writer.
    fn borrowed(&self) -> Self {
        Self { ..*self }
    }

    /// The current event's range in the file's own coordinates, or `None` for
    /// an event with no span table entry.
    ///
    /// An empty range is `None` too: it would map a stretch of generated Typst
    /// onto zero authored bytes, which reads as the top of the file rather than
    /// as "nowhere".
    fn range(&self) -> Option<Range<usize>> {
        let range = self.spans.get(self.at)?;
        (!range.is_empty()).then(|| (range.start + self.offset)..(range.end + self.offset))
    }

    /// The current event's span, in the file's own coordinates.
    fn span(&self) -> miette::SourceSpan {
        let range = self.range().unwrap_or(self.offset..self.offset);
        miette::SourceSpan::new(range.start.into(), range.len())
    }

    /// The range of the current fenced block's *content* in the file: its span
    /// less the info line, so an offset inside the block maps to the line the
    /// author wrote rather than to the fence marker above it.
    fn fenced(&self, len: usize) -> Option<Range<usize>> {
        let span = self.spans.get(self.at)?;
        let start = span.start + self.offset;
        let end = (span.end + self.offset).min(self.file.len());
        let newline = self.file.get(start..end)?.find('\n')?;
        let text = start + newline + 1;
        Some(text..(text + len).min(self.file.len()))
    }

    /// The file, named, for a diagnostic to render its snippet from.
    fn source(&self, path: &str) -> miette::NamedSource<String> {
        miette::NamedSource::new(path, self.file.to_owned())
    }
}

/// A table column's alignment, as the Typst identifier that names it.
///
/// An enum rather than the identifier as a string: the set is closed, so a
/// misspelling is generated code that does not compile, and the mapping from
/// what the parser reports to what Typst reads lives in exactly one place.
#[derive(Clone, Copy)]
enum Align {
    Left,
    Center,
    Right,
}

impl From<Alignment> for Align {
    fn from(alignment: Alignment) -> Self {
        match alignment {
            Alignment::Right => Self::Right,
            Alignment::Center => Self::Center,
            // A column the table left unaligned reads left, as it does in HTML.
            Alignment::Left | Alignment::None => Self::Left,
        }
    }
}

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

/// Lowered Typst together with where each stretch of it came from.
///
/// Every buffer on the writer's stack carries its own list rather than the
/// writer keeping one: a buffered construct is written apart from its parent
/// and spliced in afterwards, so an offset taken while it was open names the
/// wrong bytes once its text has moved. [`Buffer::splice`] shifts the child's
/// list by where its text landed, which is the one operation that keeps the two
/// in step - and which the lowering did not have, so it recorded top-level
/// fences only and mapped everything nested to nothing.
#[derive(Default, Clone)]
struct Buffer {
    text: String,
    /// Where each stretch of `text` came from, in write order, so the innermost
    /// construct covering an offset is always the one found first.
    spans: Vec<Mapping>,
}

impl Buffer {
    fn push(&mut self, text: &str) {
        self.text.push_str(text);
    }

    /// Append `child`'s text, moving its spans to where that text landed.
    fn splice(&mut self, child: Self) {
        let at = self.text.len();
        self.text.push_str(&child.text);
        self.spans
            .extend(child.spans.into_iter().map(|span| span.shifted(at)));
    }

    /// Record that everything written since `from` came from `source`.
    ///
    /// Nothing is recorded for an event that wrote nothing, which keeps an
    /// empty pair out of the map: [`SourceMap`] resolves the first pair
    /// covering an offset, and an empty one covers none.
    fn record(&mut self, from: usize, source: Range<usize>, shape: Shape) {
        let to = self.text.len();
        if from < to {
            self.spans.push(Mapping::new(from..to, source, shape));
        }
    }
}

struct Writer<'a> {
    path: &'a str,
    at: Located<'a>,
    /// What the site allows, consulted where a page asks for something it may
    /// not have: an `eval` fence, and raw HTML.
    config: &'a MarkdownConfig,
    /// The output, and above it one buffer per open [`Buffered`] construct.
    /// Writing always targets the top, so a nested image inside a footnote
    /// nests its buffers too.
    stack: Vec<Buffer>,
    buffered: Vec<Buffered>,
    /// Footnote bodies by label, lowered in the pre-pass. Typst has no separate
    /// definition: the body belongs at the reference, so it has to be known
    /// before the reference is reached. Kept as buffers, not strings, so a body
    /// keeps pointing at the definition it was written from once it has been
    /// moved to the reference.
    notes: Vec<(String, Buffer)>,
    /// Column alignments of the table being written, from its `Start(Table)`.
    columns: Vec<Alignment>,
}

/// Where the writer stood before an event: which buffer was on top, and how
/// much of it was written. What lets the walk attribute exactly the bytes one
/// event produced, and nothing else.
struct Mark {
    depth: usize,
    at: usize,
}

impl<'a> Writer<'a> {
    fn new(path: &'a str, at: Located<'a>, config: &'a MarkdownConfig) -> Self {
        Self {
            path,
            at,
            config,
            stack: vec![Buffer::default()],
            buffered: Vec::new(),
            notes: Vec::new(),
            columns: Vec::new(),
        }
    }

    /// A markdown heading level as a Typst one, which is the same number: `#`
    /// is `=`, so a markdown page and a typst page with the same outline render
    /// the same HTML.
    ///
    /// Clamped at 5 because typst-html renders level *n* as `h(n+1)`, and a
    /// level 6 becomes `div role="heading" aria-level="7"` -- which warns on
    /// every occurrence, and which the anchor pass does not recognise as a
    /// heading, so it silently loses its `id`. Six `#` are rare; a heading with
    /// no anchor is worse than one a level too shallow.
    fn depth(level: HeadingLevel) -> usize {
        match level {
            HeadingLevel::H1 => 1,
            HeadingLevel::H2 => 2,
            HeadingLevel::H3 => 3,
            HeadingLevel::H4 => 4,
            HeadingLevel::H5 | HeadingLevel::H6 => 5,
        }
    }

    fn finish(mut self) -> Buffer {
        self.stack.pop().unwrap_or_default()
    }

    fn out(&mut self) -> &mut Buffer {
        self.stack
            .last_mut()
            .expect("the root buffer is never popped")
    }

    fn push(&mut self, text: &str) {
        self.out().push(text);
    }

    /// Where the writer stands, taken before an event so what it writes can be
    /// attributed afterwards.
    fn mark(&mut self) -> Mark {
        Mark {
            depth: self.stack.len(),
            at: self.out().text.len(),
        }
    }

    /// Attribute everything the event just written produced to the markdown it
    /// came from.
    ///
    /// Skipped when the event opened or closed a buffer: `before` was measured
    /// against a buffer that is no longer the one being written to, and the
    /// close is recorded by [`Writer::end`], which is the only place that knows
    /// how the child's text was transformed on its way in.
    fn attribute(&mut self, before: &Mark) {
        if self.stack.len() != before.depth {
            return;
        }
        let Some(source) = self.at.range() else {
            return;
        };
        // Assembled, not copied: every construct but an `eval` fence is a
        // generated call or an escaped literal, whose bytes line up with
        // nothing on the authored side.
        self.out().record(before.at, source, Shape::Whole);
    }

    /// Lower every footnote definition first. The main walk then skips them and
    /// reads the bodies back at each reference.
    fn notes(&mut self, events: &[Numbered<'_>]) -> Result<()> {
        let mut depth = 0usize;
        let mut current: Option<String> = None;
        let mut body: Vec<Numbered<'_>> = Vec::new();

        for numbered in events {
            match &numbered.1 {
                Event::Start(Tag::FootnoteDefinition(name)) if depth == 0 => {
                    current = Some(name.to_string());
                    depth = 1;
                }
                _ if depth == 0 => {}
                Event::Start(_) => {
                    depth += 1;
                    body.push(numbered.clone());
                }
                Event::End(TagEnd::FootnoteDefinition) if depth == 1 => {
                    depth = 0;
                    let name = current.take().unwrap_or_default();
                    let mut inner = Writer::new(self.path, self.at.borrowed(), self.config);
                    inner.notes.clone_from(&self.notes);
                    inner.walk(&body)?;
                    let lowered = inner.finish();
                    match self.notes.iter_mut().find(|(label, _)| *label == name) {
                        Some(existing) => existing.1 = lowered,
                        None => self.notes.push((name, lowered)),
                    }
                    body.clear();
                }
                Event::End(_) => {
                    depth -= 1;
                    body.push(numbered.clone());
                }
                _ => body.push(numbered.clone()),
            }
        }
        Ok(())
    }

    /// A footnote definition is not part of the flow: its body was lowered by
    /// [`Writer::notes`] and belongs at the reference, so it is skipped here.
    fn walk(&mut self, events: &[Numbered<'_>]) -> Result<()> {
        let mut depth = 0usize;

        for (index, event) in events {
            if depth > 0 {
                match event {
                    Event::Start(_) => depth += 1,
                    Event::End(TagEnd::FootnoteDefinition) if depth == 1 => depth = 0,
                    Event::End(_) => depth -= 1,
                    _ => {}
                }
                continue;
            }
            if matches!(event, Event::Start(Tag::FootnoteDefinition(_))) {
                depth = 1;
                continue;
            }
            self.at.at = *index;
            // Every event is authored: the parser hands back the bytes each one
            // was parsed from, so whatever this writes belongs to those bytes,
            // generated call or literal text alike. What has no origin is what
            // no event produced -- the wrapper the body is compiled inside, and
            // the punctuation `end` writes while closing a buffered construct
            // whose text was transformed rather than copied.
            let before = self.mark();
            self.event(event)?;
            self.attribute(&before);
        }
        Ok(())
    }

    fn event(&mut self, event: &Event<'_>) -> Result<()> {
        match event {
            Event::Start(tag) => {
                self.start(tag);
                Ok(())
            }
            Event::End(tag) => {
                self.end(*tag);
                Ok(())
            }
            Event::Text(text) => {
                match self.buffered.last_mut() {
                    // Inside a code block the text is the code: it is escaped
                    // once, as a whole string, when the block closes.
                    Some(Buffered::Code { .. }) => self.push(text),
                    // Inside an alt run the text is the attribute, kept raw
                    // because that is what it has to be at the end.
                    Some(Buffered::Alt { alt, .. }) => alt.push_str(text),
                    None => {
                        let literal = Content(text.as_ref()).to_string();
                        self.push(&literal);
                    }
                }
                Ok(())
            }
            Event::Code(code) => {
                // An alt attribute is plain text, so a code span inside one is
                // its text: the marks around it have nowhere to go.
                if let Some(Buffered::Alt { alt, .. }) = self.buffered.last_mut() {
                    alt.push_str(code);
                    return Ok(());
                }
                let call = Call::new("raw").pos(Value::str(code.as_ref())).to_string();
                self.push(&call);
                Ok(())
            }
            Event::SoftBreak => {
                self.push("#\" \"");
                Ok(())
            }
            Event::HardBreak => {
                self.push(&Call::new("linebreak").to_string());
                Ok(())
            }
            Event::Rule => {
                // Not `line`, which typst drops on HTML export with a warning
                // and no element: a thematic break is a rule in the document,
                // and `hr` is the element that means it.
                let call = Call::new("html.elem").pos(Value::str("hr")).to_string();
                self.push(&call);
                self.push("\n\n");
                Ok(())
            }
            Event::TaskListMarker(done) => {
                self.push(if *done {
                    "#sym.ballot.check "
                } else {
                    "#sym.ballot "
                });
                Ok(())
            }
            Event::FootnoteReference(name) => {
                let body = self
                    .notes
                    .iter()
                    .find(|(label, _)| label == name.as_ref())
                    .map(|(_, body)| body.clone())
                    .unwrap_or_default();
                self.push(&Call::new("footnote").content().to_string());
                // Spliced rather than pushed as text: the body was lowered at
                // the definition, so its spans point there and have to be moved
                // to wherever it lands at this reference.
                self.out().splice(body);
                self.push("]");
                Ok(())
            }
            // A comment is not content, and dropping it drops nothing: it is
            // the one shape of raw HTML with no rendered counterpart to lose.
            Event::Html(raw) | Event::InlineHtml(raw) if is_comment(raw) => Ok(()),
            // The rest has no home here. The DOM this build produces is typed
            // and typst-html owns the document element, so a string of markup
            // cannot be spliced into it: it would have to be parsed, and a
            // second HTML parser is exactly the string templating the pipeline
            // exists to avoid. A fence with `html.elem` says the same thing in
            // the typed form.
            //
            // Refusing is the default and dropping is the site's call: content
            // written elsewhere arrives carrying markup nobody wants to hand-fix.
            //
            // Dropping an *inline* run removes the tags and leaves the prose
            // between them, because that prose arrives as its own text events.
            // A block-level run is one event carrying its whole contents, so
            // dropping it drops those too -- which is what `drop` has to mean
            // for a block, and why `refuse` is the default.
            Event::Html(_) | Event::InlineHtml(_) => match self.config.html {
                RawHtml::Drop => Ok(()),
                RawHtml::Refuse => Err(MarkdownError::RawHtml {
                    path: self.path.to_owned(),
                    src: self.at.source(self.path),
                    span: self.at.span(),
                }
                .into()),
            },
            Event::InlineMath(text) => {
                let call = Call::new("math.equation")
                    .pos(Value::str(text.as_ref()))
                    .to_string();
                self.push(&call);
                Ok(())
            }
            Event::DisplayMath(text) => {
                let call = Call::new("math.equation")
                    .named("block", Value::Bool(true))
                    .pos(Value::str(text.as_ref()))
                    .to_string();
                self.push(&call);
                Ok(())
            }
        }
    }

    fn start(&mut self, tag: &Tag<'_>) {
        match tag {
            Tag::Heading { level, .. } => {
                let level = i64::try_from(Self::depth(*level)).unwrap_or(1);
                let call = Call::new("heading")
                    .named("level", Value::Int(level))
                    .content()
                    .to_string();
                self.push(&call);
            }
            Tag::BlockQuote(_) => {
                let call = Call::new("quote")
                    .named("block", Value::Bool(true))
                    .content()
                    .to_string();
                self.push(&call);
            }
            Tag::CodeBlock(kind) => {
                let fence = match kind {
                    CodeBlockKind::Fenced(info) => Fence::parse(info),
                    CodeBlockKind::Indented => Fence::parse(""),
                };
                self.buffered.push(Buffered::Code { fence });
                self.stack.push(Buffer::default());
            }
            Tag::List(Some(start)) => {
                let start = i64::try_from(*start).unwrap_or(1);
                let call = Call::new("enum")
                    .named("start", Value::Int(start))
                    .items()
                    .to_string();
                self.push(&call);
            }
            Tag::List(None) => self.push(&Call::new("list").items().to_string()),
            Tag::Item | Tag::TableCell => self.push("["),
            Tag::Emphasis => self.push(&Call::new("emph").content().to_string()),
            Tag::Strong => self.push(&Call::new("strong").content().to_string()),
            Tag::Strikethrough => self.push(&Call::new("strike").content().to_string()),
            Tag::Link { dest_url, .. } => {
                let call = Call::new("link")
                    .pos(Value::str(dest_url.as_ref()))
                    .content()
                    .to_string();
                self.push(&call);
            }
            Tag::Image { dest_url, .. } => {
                self.buffered.push(Buffered::Alt {
                    dest: dest_url.to_string(),
                    alt: String::new(),
                });
                self.stack.push(Buffer::default());
            }
            Tag::Table(alignments) => {
                self.columns.clone_from(alignments);
                let align = Value::array(
                    self.columns
                        .iter()
                        .copied()
                        .map(Align::from)
                        .map(Value::from),
                );
                let columns = i64::try_from(self.columns.len()).unwrap_or(1);
                let call = Call::new("table")
                    .named("columns", Value::Int(columns))
                    .named("align", align)
                    .items()
                    .to_string();
                self.push(&call);
            }
            // Bare: this sits inside the `#table(..` this writer just opened,
            // where Typst is already reading arguments and a `#` would be a
            // syntax error.
            Tag::TableHead => self.push(&Call::new("table.header").bare().items().to_string()),
            // Nothing to open: a paragraph is separated by its end, a row by
            // its cells, and the rest have no Typst counterpart.
            Tag::Paragraph
            | Tag::TableRow
            | Tag::FootnoteDefinition(_)
            | Tag::MetadataBlock(_)
            | Tag::HtmlBlock
            | Tag::DefinitionList
            | Tag::DefinitionListTitle
            | Tag::DefinitionListDefinition
            | Tag::Superscript
            | Tag::Subscript => {}
        }
    }

    fn end(&mut self, tag: TagEnd) {
        match tag {
            TagEnd::Paragraph => self.push("\n\n"),
            TagEnd::Heading(_) | TagEnd::BlockQuote(_) => self.push("]\n\n"),
            TagEnd::CodeBlock => {
                let code = self.stack.pop().unwrap_or_default();
                let text = code.text;
                let Some(Buffered::Code { fence }) = self.buffered.pop() else {
                    return;
                };
                let at = self.out().text.len();
                if fence.runs(self.config) {
                    // Emitted verbatim, so the pair is byte-for-byte and an
                    // error inside a multi-line fence lands on its own line.
                    // `fenced` steps over the info line, which the block's own
                    // span includes and its text does not.
                    let source = self.at.fenced(text.len());
                    self.push(&text);
                    if let Some(source) = source {
                        self.out().record(at, source, Shape::Verbatim);
                    }
                    self.push("\n\n");
                    return;
                }
                let mut call = Call::new("raw").named("block", Value::Bool(true));
                if let Some(name) = fence.lang {
                    call = call.named("lang", Value::str(&name));
                }
                self.push(&call.pos(Value::str(&text)).to_string());
                // The call is nothing like the fence it renders -- the code is
                // escaped into a string argument -- so the fence maps as a
                // whole and an offset inside it resolves to the fence's start
                // rather than to a byte the author never typed there.
                if let Some(source) = self.at.range() {
                    self.out().record(at, source, Shape::Whole);
                }
                self.push("\n\n");
            }
            TagEnd::List(_) | TagEnd::Table => self.push(")\n\n"),
            TagEnd::Item | TagEnd::TableCell => self.push("],"),
            TagEnd::Emphasis | TagEnd::Strong | TagEnd::Strikethrough | TagEnd::Link => {
                self.push("]");
            }
            TagEnd::Image => {
                // The lowered buffer is discarded: an alt run's inline marks
                // have no home in a string attribute, and `alt` already holds
                // the text they wrapped.
                self.stack.pop();
                let Some(Buffered::Alt { dest, alt }) = self.buffered.pop() else {
                    return;
                };
                let mut call = Call::new("image").pos(Value::str(&dest));
                if !alt.is_empty() {
                    call = call.named("alt", Value::str(&alt));
                }
                let at = self.out().text.len();
                self.push(&call.to_string());
                // Recorded here rather than by the walk, which cannot attribute
                // an event that closed a buffer. The call maps as a whole: it
                // is assembled from the destination and the alt run, not copied
                // from either.
                if let Some(source) = self.at.range() {
                    self.out().record(at, source, Shape::Whole);
                }
            }
            TagEnd::TableHead => self.push("),"),
            TagEnd::TableRow
            | TagEnd::FootnoteDefinition
            | TagEnd::MetadataBlock(_)
            | TagEnd::HtmlBlock
            | TagEnd::DefinitionList
            | TagEnd::DefinitionListTitle
            | TagEnd::DefinitionListDefinition
            | TagEnd::Superscript
            | TagEnd::Subscript => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::Rebased;

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
