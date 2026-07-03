use miette::Diagnostic;
use thiserror::Error;
use typst::diag::SourceDiagnostic;
use typst::ecow::EcoVec;

use crate::error::Annotated;

#[derive(Error, Debug)]
#[error("{kind}")]
pub struct ContentError {
    kind: ContentErrorKind,
}

impl ContentError {
    pub fn frontmatter_eval(src: &str, errs: EcoVec<SourceDiagnostic>) -> Self {
        let diags: Vec<_> = errs
            .into_iter()
            .map(|e| FrontmatterDiag::new(src, e))
            .collect();
        Self {
            kind: ContentErrorKind::FrontmatterEval {
                src: src.to_owned(),
                errs: diags,
            },
        }
    }

    /// Lower wax's own span-annotated glob error into an [`Annotated`] so its
    /// labels point straight at the offending part of the pattern.
    pub fn bad_glob(pattern: &str, error: wax::BuildError) -> Self {
        let mut diag = Annotated::new(
            "baudelaire::content::bad_glob",
            format!("invalid collection glob `{pattern}`"),
            pattern.to_owned(),
        )
        .help(error.to_string());
        for location in error.locations() {
            let (offset, len) = location.span();
            diag = diag.label(location.to_string(), offset, len);
        }
        Self {
            kind: ContentErrorKind::BadGlob(diag),
        }
    }

    pub fn frontmatter_not_dict(src: &str, value: typst::foundations::Value) -> Self {
        use typst::foundations::Repr;
        Self {
            kind: ContentErrorKind::FrontmatterNotDict {
                src: src.to_owned(),
                ty: value.ty().long_name(),
                repr: value.repr().to_string(),
            },
        }
    }
}

impl miette::Diagnostic for ContentError {
    fn code(&self) -> Option<Box<dyn std::fmt::Display + '_>> {
        self.kind.code()
    }

    fn severity(&self) -> Option<miette::Severity> {
        self.kind.severity()
    }

    fn help(&self) -> Option<Box<dyn std::fmt::Display + '_>> {
        self.kind.help()
    }

    fn source_code(&self) -> Option<&dyn miette::SourceCode> {
        self.kind.source_code()
    }

    fn labels(&self) -> Option<Box<dyn Iterator<Item = miette::LabeledSpan> + '_>> {
        self.kind.labels()
    }

    fn diagnostic_source(&self) -> Option<&dyn miette::Diagnostic> {
        self.kind.diagnostic_source()
    }

    fn related(&self) -> Option<Box<dyn Iterator<Item = &'_ dyn miette::Diagnostic> + '_>> {
        match &self.kind {
            ContentErrorKind::FrontmatterEval { errs, .. } => {
                Some(Box::new(errs.iter().map(|d| d as &dyn miette::Diagnostic)))
            }
            _ => None,
        }
    }
}

#[derive(Error, Diagnostic, Debug)]
pub enum ContentErrorKind {
    #[error("failed to evaluate frontmatter")]
    #[diagnostic(
        code(baudelaire::content::frontmatter_eval),
        help("frontmatter must be a typst dict literal of plain data")
    )]
    FrontmatterEval {
        #[source_code]
        src: String,
        #[related]
        errs: Vec<FrontmatterDiag>,
    },

    #[error(transparent)]
    #[diagnostic(transparent)]
    BadGlob(Annotated),

    #[error("frontmatter must be a dictionary, but got a {ty}: {repr}")]
    #[diagnostic(
        code(baudelaire::content::frontmatter_not_dict),
        help("wrap the frontmatter fields in `(key: value, ...)`")
    )]
    FrontmatterNotDict {
        #[source_code]
        src: String,
        ty: &'static str,
        repr: String,
    },
}

/// Bridge typst's miette-5 [`SourceDiagnostic`] to our miette-7.
#[derive(Debug, Clone)]
pub struct FrontmatterDiag {
    src: String,
    inner: SourceDiagnostic,
}

impl FrontmatterDiag {
    fn new(src: &str, inner: SourceDiagnostic) -> Self {
        Self {
            src: src.to_owned(),
            inner,
        }
    }
}

impl std::fmt::Display for FrontmatterDiag {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.inner.message)
    }
}

impl std::error::Error for FrontmatterDiag {}

impl miette::Diagnostic for FrontmatterDiag {
    fn code(&self) -> Option<Box<dyn std::fmt::Display + '_>> {
        Some(Box::new("typst::frontmatter"))
    }

    fn severity(&self) -> Option<miette::Severity> {
        Some(match self.inner.severity {
            typst::diag::Severity::Error => miette::Severity::Error,
            typst::diag::Severity::Warning => miette::Severity::Warning,
        })
    }

    fn source_code(&self) -> Option<&dyn miette::SourceCode> {
        Some(&self.src as &dyn miette::SourceCode)
    }
}

impl From<ContentError> for crate::error::BaudelaireErrorKind {
    fn from(e: ContentError) -> Self {
        Self::Content(Box::new(e))
    }
}
