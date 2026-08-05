//! Writing the Typst: the event walk, its output buffer, and the source map it records.

use super::Numbered;
use super::fence::{Buffered, Fence, is_comment};
use super::located::Located;
use crate::codegen::{Call, Content, Value};
use crate::config::{MarkdownConfig, RawHtml};
use crate::content::sourcemap::{Mapping, Shape};
use crate::error::Result;
use crate::error::markdown::MarkdownError;
use pulldown_cmark::{Alignment, CodeBlockKind, Event, HeadingLevel, Tag, TagEnd};
use std::ops::Range;
/// A table column's alignment, as the Typst identifier that names it.
///
/// An enum rather than the identifier as a string: the set is closed, so a
/// misspelling is generated code that does not compile, and the mapping from
/// what the parser reports to what Typst reads lives in exactly one place.
#[derive(Clone, Copy)]
pub(super) enum Align {
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
pub(super) struct Buffer {
    pub(super) text: String,
    /// Where each stretch of `text` came from, in write order, so the innermost
    /// construct covering an offset is always the one found first.
    pub(super) spans: Vec<Mapping>,
}

impl Buffer {
    pub(super) fn push(&mut self, text: &str) {
        self.text.push_str(text);
    }

    /// Append `child`'s text, moving its spans to where that text landed.
    pub(super) fn splice(&mut self, child: Self) {
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
    pub(super) fn record(&mut self, from: usize, source: Range<usize>, shape: Shape) {
        self.record_range(from..self.text.len(), source, shape);
    }

    /// Record a pair whose lowered end is known rather than "wherever the
    /// buffer has reached".
    ///
    /// What a construct emitting *several* pairs needs: [`Buffer::record`]
    /// closes each at the buffer's current end, so the first of two mappings
    /// swallowed the second and every offset inside it resolved against the
    /// first one's source line.
    pub(super) fn record_range(
        &mut self,
        lowered: Range<usize>,
        source: Range<usize>,
        shape: Shape,
    ) {
        if !lowered.is_empty() {
            self.spans.push(Mapping::new(lowered, source, shape));
        }
    }
}
pub(super) struct Writer<'a> {
    pub(super) path: &'a str,
    pub(super) at: Located<'a>,
    /// What the site allows, consulted where a page asks for something it may
    /// not have: an `eval` fence, and raw HTML.
    pub(super) config: &'a MarkdownConfig,
    /// The output, and above it one buffer per open [`Buffered`] construct.
    /// Writing always targets the top, so a nested image inside a footnote
    /// nests its buffers too.
    pub(super) stack: Vec<Buffer>,
    pub(super) buffered: Vec<Buffered>,
    /// Footnote bodies by label, lowered in the pre-pass. Typst has no separate
    /// definition: the body belongs at the reference, so it has to be known
    /// before the reference is reached. Kept as buffers, not strings, so a body
    /// keeps pointing at the definition it was written from once it has been
    /// moved to the reference.
    pub(super) notes: Vec<(String, Buffer)>,
    /// Column alignments of the table being written, from its `Start(Table)`.
    pub(super) columns: Vec<Alignment>,
}

/// Where the writer stood before an event: which buffer was on top, and how
/// much of it was written. What lets the walk attribute exactly the bytes one
/// event produced, and nothing else.
pub(super) struct Mark {
    pub(super) depth: usize,
    pub(super) at: usize,
}
impl<'a> Writer<'a> {
    pub(super) fn new(path: &'a str, at: Located<'a>, config: &'a MarkdownConfig) -> Self {
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
    pub(super) fn depth(level: HeadingLevel) -> usize {
        match level {
            HeadingLevel::H1 => 1,
            HeadingLevel::H2 => 2,
            HeadingLevel::H3 => 3,
            HeadingLevel::H4 => 4,
            HeadingLevel::H5 | HeadingLevel::H6 => 5,
        }
    }

    pub(super) fn finish(mut self) -> Buffer {
        self.stack.pop().unwrap_or_default()
    }

    pub(super) fn out(&mut self) -> &mut Buffer {
        self.stack
            .last_mut()
            .expect("the root buffer is never popped")
    }

    pub(super) fn push(&mut self, text: &str) {
        self.out().push(text);
    }

    /// Where the writer stands, taken before an event so what it writes can be
    /// attributed afterwards.
    pub(super) fn mark(&mut self) -> Mark {
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
    pub(super) fn attribute(&mut self, before: &Mark) {
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
    pub(super) fn notes(&mut self, events: &[Numbered<'_>]) -> Result<()> {
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
    pub(super) fn walk(&mut self, events: &[Numbered<'_>]) -> Result<()> {
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

    pub(super) fn event(&mut self, event: &Event<'_>) -> Result<()> {
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

    pub(super) fn start(&mut self, tag: &Tag<'_>) {
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

    pub(super) fn end(&mut self, tag: TagEnd) {
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
                    // Emitted verbatim, so an error inside a multi-line fence
                    // lands on its own line. Paired a line at a time rather than
                    // as one block: an indented fence (inside a list item, a
                    // blockquote) reaches here with its indentation already
                    // stripped, so only the lines correspond byte for byte.
                    let lines = self.at.fenced_lines(&text);
                    self.push(&text);
                    for (lowered, source) in lines {
                        self.out().record_range(
                            at + lowered.start..at + lowered.end,
                            source,
                            Shape::Verbatim,
                        );
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
