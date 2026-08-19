use std::sync::Arc;

use itertools::Itertools;
use miette::NamedSource;
use typst::{
    World, WorldExt,
    diag::SourceDiagnostic,
    syntax::{DiagSpan, FileId},
};

use crate::content::{Rebased, SourceMap};
use crate::ui::{Code, Text};

/// A typst diagnostic bridged to miette, with span resolution via the world
/// that produced it.
///
/// A label is drawn only for spans in the one file [`src`](Self::src) holds: a
/// span reaching into another file would overrun that text and panic miette
/// with `OutOfBounds`.
pub struct TypstSourceDiagnostic {
    inner: SourceDiagnostic,
    src: NamedSource<String>,
    file: Option<FileId>,
    world: Arc<dyn World + Send + Sync>,
    /// Set only for a page lowered from another language whose own span was
    /// authored: then `src` is that authored file, and a label that does not
    /// translate into it is dropped rather than drawn at an offset measured
    /// against a different text.
    rebased: Option<Rebased>,
}

impl TypstSourceDiagnostic {
    pub fn new(
        inner: SourceDiagnostic,
        src: NamedSource<String>,
        file: Option<FileId>,
        world: Arc<dyn World + Send + Sync>,
        rebased: Option<Rebased>,
    ) -> Self {
        Self {
            inner,
            src,
            file,
            world,
            rebased,
        }
    }

    /// Bridge a batch of typst diagnostics, resolving each against the file its
    /// span belongs to (a bound template, a shared module, the page itself) so
    /// the snippet always matches the span. Spanless diagnostics fall back to
    /// the `fallback` name and text.
    ///
    /// Only a span in the main file is translated through `sourcemap`:
    /// [`Rebased::new`] cannot tell a bound template from the page it was bound
    /// into, and would report a template's typo against a line of the page's
    /// prose.
    pub fn bridge(
        errs: impl IntoIterator<Item = SourceDiagnostic>,
        fallback: (&str, &str),
        world: Arc<dyn World + Send + Sync>,
        sourcemap: Option<(&Arc<SourceMap>, &str)>,
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
                let mapped = sourcemap.and_then(|(map, name)| {
                    let id = file?;
                    if id != world.main() {
                        return None;
                    }
                    let wrapper = world.source(id).ok()?;
                    let rebased = Rebased::new(Arc::clone(map), wrapper.text())?;
                    let range = world.range(e.span)?;
                    rebased.locate(&range)?;
                    Some((NamedSource::new(name, map.text().to_owned()), rebased))
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
        if span.id() != self.file {
            return None;
        }
        let range = self.world.range(span)?;
        let range = match &self.rebased {
            Some(rebased) => rebased.locate(&range)?,
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

/// typst's own message, escaped: a compiler quoting an identifier in backticks
/// is not writing this crate's markup.
impl std::fmt::Display for TypstSourceDiagnostic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", Text(&self.inner.message))
    }
}

impl std::error::Error for TypstSourceDiagnostic {}

impl miette::Diagnostic for TypstSourceDiagnostic {
    fn code(&self) -> Option<Box<dyn std::fmt::Display + '_>> {
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
        let trace = self
            .inner
            .trace
            .iter()
            .filter_map(|frame| self.labeled(frame.span, Some(&frame.v.to_string())));
        let labels: Vec<_> = main.chain(hints).chain(trace).collect();
        (!labels.is_empty()).then(|| Box::new(labels.into_iter()) as _)
    }
}

/// A project file typst's own store could not hand back, in the shape the store
/// reported it rather than as a generic read failure.
///
/// Every [`FileError`](typst::diag::FileError) shape is named, so a typst bump
/// that adds one fails to compile here rather than falling into a bucket.
#[derive(Debug, thiserror::Error, miette::Diagnostic)]
pub enum TypstFileError {
    #[error("{} does not exist", Code(.path))]
    #[diagnostic(
        code(baudelaire::typst::file::missing),
        help("check the spelling, and that whatever writes it runs before the page that reads it")
    )]
    Missing { path: String },

    #[error("{} could not be opened", Code(.path))]
    #[diagnostic(
        code(baudelaire::typst::file::denied),
        help("permission denied: check the file's ownership and mode")
    )]
    Denied { path: String },

    #[error("{} is a directory, and a file was expected", Code(.path))]
    #[diagnostic(code(baudelaire::typst::file::directory))]
    Directory { path: String },

    #[error("{} is not a typst source file", Code(.path))]
    #[diagnostic(
        code(baudelaire::typst::file::not_source),
        help("a module is read from a `.typ` file")
    )]
    NotSource { path: String },

    #[error("{} is not valid UTF-8", Code(.path))]
    #[diagnostic(
        code(baudelaire::typst::file::encoding),
        help("typst reads a source file as UTF-8; re-save it in that encoding")
    )]
    Encoding { path: String },

    #[error("{} could not be resolved on this platform: {}", Code(.path), Text(.detail))]
    #[diagnostic(code(baudelaire::typst::file::realize))]
    Realize { path: String, detail: String },

    #[error("the package {} is part of could not be loaded: {}", Code(.path), Text(.detail))]
    #[diagnostic(
        code(baudelaire::typst::file::package),
        help("check `typst {{ package {{ }} }}` and that the registry is reachable")
    )]
    Package { path: String, detail: String },

    #[error("{} could not be read: {}", Code(.path), Text(.detail))]
    #[diagnostic(code(baudelaire::typst::file::other))]
    Other { path: String, detail: String },
}

impl TypstFileError {
    /// The failure typst reported for `path`, as this crate's own diagnostic.
    pub fn of(path: &std::path::Path, error: &typst::diag::FileError) -> Self {
        use typst::diag::FileError;

        let path = path.display().to_string();
        match error {
            FileError::NotFound(_) => Self::Missing { path },
            FileError::AccessDenied => Self::Denied { path },
            FileError::IsDirectory => Self::Directory { path },
            FileError::NotSource => Self::NotSource { path },
            FileError::InvalidUtf8 => Self::Encoding { path },
            FileError::Realize(e) => Self::Realize {
                path,
                detail: e.to_string(),
            },
            FileError::Package(e) => Self::Package {
                path,
                detail: e.to_string(),
            },
            FileError::Other(detail) => Self::Other {
                path,
                detail: detail
                    .as_ref()
                    .map_or_else(|| "no further detail".to_owned(), ToString::to_string),
            },
        }
    }
}
