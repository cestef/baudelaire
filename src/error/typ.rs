use std::sync::Arc;

use itertools::Itertools;
use miette::NamedSource;
use typst::{
    World, WorldExt,
    diag::SourceDiagnostic,
    syntax::{DiagSpan, FileId},
};

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
}

impl TypstSourceDiagnostic {
    pub fn new(
        inner: SourceDiagnostic,
        src: NamedSource<String>,
        file: Option<FileId>,
        world: Arc<dyn World + Send + Sync>,
    ) -> Self {
        Self {
            inner,
            src,
            file,
            world,
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
    ) -> Vec<Self> {
        errs.into_iter()
            .map(|e| {
                let file = e.span.id();
                let src = file
                    .and_then(|id| world.source(id).ok().map(|src| (id, src)))
                    .map(|(id, src)| {
                        let name = id.vpath().get_without_slash().to_string();
                        NamedSource::new(name, src.text().to_owned())
                    })
                    .unwrap_or_else(|| NamedSource::new(fallback.0, fallback.1.to_owned()));
                Self::new(e, src, file, world.clone())
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

impl std::fmt::Display for TypstSourceDiagnostic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.inner.message)
    }
}

impl std::error::Error for TypstSourceDiagnostic {}

impl miette::Diagnostic for TypstSourceDiagnostic {
    fn code(&self) -> Option<Box<dyn std::fmt::Display + '_>> {
        Some(Box::new(match self.inner.severity {
            typst::diag::Severity::Error => "typst::error",
            typst::diag::Severity::Warning => "typst::warning",
        }))
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
            Box::new(helps.iter().map(|e| &e.v).join("\n")) as Box<dyn std::fmt::Display + '_>
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
