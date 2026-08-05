use std::sync::Arc;

use itertools::Itertools;
use miette::NamedSource;
use typst::{
    World, WorldExt,
    diag::SourceDiagnostic,
    syntax::{DiagSpan, FileId},
};

use std::ops::Range;

use crate::ui::Text;

/// Where the Typst a page lowered to came from in the file its author wrote.
///
/// Only spans of *authored* Typst are recorded -- the inside of an `eval` fence,
/// and nothing else. That is deliberate rather than partial: the rest of the
/// lowering is machine-generated, so an error in it is this crate's bug, and
/// pointing the author at their own file for it would be a lie.
#[derive(Debug, Clone, Default, Hash, PartialEq, Eq)]
pub struct Origins {
    /// The authored file, for the snippet a diagnostic renders.
    text: String,
    /// Byte length of the lowered body. The body is written last, so it sits at
    /// the end of the wrapper and everything before it is the preamble.
    body: usize,
    /// `(range in the lowered body, range in `text`)`.
    spans: Vec<(Range<usize>, Range<usize>)>,
}

impl Origins {
    /// `spans` pairs a range of the generated body with the range of `text` it
    /// was written from. `body` is that body's length.
    pub fn new(text: String, body: usize, spans: Vec<(Range<usize>, Range<usize>)>) -> Self {
        Self { text, body, spans }
    }

    /// Where `range` -- a span in the *wrapper* Typst that was compiled -- came
    /// from in [`text`](Self::text), if it came from there at all.
    ///
    /// `wrapper` is that whole text's length, which is what locates the body
    /// inside it. `None` for a span in generated code, and callers must keep it
    /// that way: an untranslated offset drawn against the authored file would
    /// underline an unrelated place, or overrun it.
    pub fn locate(&self, wrapper: usize, range: &Range<usize>) -> Option<Range<usize>> {
        let start = range.start.checked_sub(wrapper.checked_sub(self.body)?)?;
        let (from, to) = self
            .spans
            .iter()
            .find(|(lowered, _)| lowered.start <= start && start < lowered.end)?;
        // The offset within the span is kept, so a diagnostic inside a
        // multi-line fence lands on its own line rather than at the top of it.
        let at = to.start + (start - from.start).min(to.len());
        let end = (at + range.len()).min(to.end).max(at);
        Some(at..end)
    }
}

/// A typst diagnostic bridged to miette, with span resolution via the world
/// that produced it. [`src`](Self::src) holds one file's text; [`file`](Self::file)
/// records which file that is, so a label is only drawn for spans in that same
/// file: a span reaching into another file (a bound template, a shared module)
/// would otherwise overrun this text and panic miette with `OutOfBounds`.
pub struct TypstSourceDiagnostic {
    inner: SourceDiagnostic,
    src: NamedSource<String>,
    file: Option<FileId>,
    world: Arc<dyn World + Send + Sync>,
    /// Set only for a page lowered from another language, and only when this
    /// diagnostic's own span was written by its author. Then `src` is that
    /// authored file and every label is translated into it; a label that does
    /// not translate is dropped rather than drawn at an offset measured against
    /// a different text.
    origins: Option<(Arc<Origins>, usize)>,
}

impl TypstSourceDiagnostic {
    pub fn new(
        inner: SourceDiagnostic,
        src: NamedSource<String>,
        file: Option<FileId>,
        world: Arc<dyn World + Send + Sync>,
        origins: Option<(Arc<Origins>, usize)>,
    ) -> Self {
        Self {
            inner,
            src,
            file,
            world,
            origins,
        }
    }

    /// Bridge a batch of typst diagnostics, resolving each against the file its
    /// span belongs to (a bound template, a shared module, the page itself) so
    /// the snippet always matches the span. Spanless diagnostics fall back to
    /// the `fallback` name and text. The single conversion the engine and
    /// content discovery share.
    pub fn bridge(
        errs: impl IntoIterator<Item = SourceDiagnostic>,
        fallback: (&str, &str),
        world: Arc<dyn World + Send + Sync>,
        origins: Option<(&Arc<Origins>, &str)>,
    ) -> Vec<Self> {
        errs.into_iter()
            .map(move |e| {
                let file = e.span.id();
                let src = file
                    .and_then(|id| world.source(id).ok().map(|src| (id, src)))
                    .map_or_else(
                        || NamedSource::new(fallback.0, fallback.1.to_owned()),
                        |(id, src)| {
                            let name = id.vpath().get_without_slash().to_owned();
                            NamedSource::new(name, src.text().to_owned())
                        },
                    );
                // Translate only when this diagnostic's own span was authored.
                // A span in generated code keeps the wrapper as its source, so
                // a lowering bug still reports somewhere real instead of
                // silently losing its position.
                let mapped = origins.and_then(|(origins, name)| {
                    let wrapper = world.source(file?).ok()?.text().len();
                    let range = world.range(e.span)?;
                    origins.locate(wrapper, &range)?;
                    Some((
                        NamedSource::new(name, origins.text.clone()),
                        (Arc::clone(origins), wrapper),
                    ))
                });
                match mapped {
                    Some((authored, translation)) => {
                        Self::new(e, authored, file, Arc::clone(&world), Some(translation))
                    }
                    None => Self::new(e, src, file, Arc::clone(&world), None),
                }
            })
            .collect()
    }

    fn labeled(
        &self,
        span: impl Into<DiagSpan>,
        label: Option<&str>,
    ) -> Option<miette::LabeledSpan> {
        let span = span.into();
        // Only spans in the file `src` holds can be measured against its text.
        if span.id() != self.file {
            return None;
        }
        let range = self.world.range(span)?;
        // With a translation in force, `src` is the authored file: a label that
        // does not map into it has no place there, so it is dropped rather than
        // drawn at an offset that means something else.
        let range = match &self.origins {
            Some((origins, wrapper)) => origins.locate(*wrapper, &range)?,
            None => range,
        };
        Some(miette::LabeledSpan::new(
            label.map(str::to_owned),
            range.start,
            range.end - range.start,
        ))
    }
}

impl std::fmt::Debug for TypstSourceDiagnostic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TypstSourceDiagnostic")
            .field("inner", &self.inner)
            .field("src", &self.src)
            .finish_non_exhaustive()
    }
}

/// typst's own message, escaped: it is foreign text, and a compiler that
/// quotes an identifier in backticks is not writing this crate's markup.
impl std::fmt::Display for TypstSourceDiagnostic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", Text(&self.inner.message))
    }
}

impl std::error::Error for TypstSourceDiagnostic {}

impl miette::Diagnostic for TypstSourceDiagnostic {
    fn code(&self) -> Option<Box<dyn std::fmt::Display + '_>> {
        // Namespaced like every other code, and keyed on the error class, not
        // on severity: severity already has its own field, and an unnamespaced
        // code is not greppable alongside the rest.
        Some(Box::new("baudelaire::typst::diagnostic"))
    }

    fn severity(&self) -> Option<miette::Severity> {
        Some(match self.inner.severity {
            typst::diag::Severity::Error => miette::Severity::Error,
            typst::diag::Severity::Warning => miette::Severity::Warning,
        })
    }

    fn help(&self) -> Option<Box<dyn std::fmt::Display + '_>> {
        let helps: Vec<_> = self
            .inner
            .hints
            .iter()
            .filter(|e| e.span.is_detached())
            .collect();
        (!helps.is_empty()).then(|| {
            Box::new(Text(helps.iter().map(|e| &e.v).join("\n"))) as Box<dyn std::fmt::Display + '_>
        })
    }

    fn source_code(&self) -> Option<&dyn miette::SourceCode> {
        Some(&self.src)
    }

    fn labels(&self) -> Option<Box<dyn Iterator<Item = miette::LabeledSpan> + '_>> {
        let main = self.labeled(self.inner.span, None).into_iter();
        let hints = self
            .inner
            .hints
            .iter()
            .filter_map(|h| self.labeled(h.span, Some(h.v.as_str())));
        // call stack leading to the error, annotated per frame: surfaces which
        // page/template a shared-module error flowed through.
        let trace = self
            .inner
            .trace
            .iter()
            .filter_map(|frame| self.labeled(frame.span, Some(&frame.v.to_string())));
        let labels: Vec<_> = main.chain(hints).chain(trace).collect();
        (!labels.is_empty()).then(|| Box::new(labels.into_iter()) as _)
    }
}
