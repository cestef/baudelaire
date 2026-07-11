use miette::Diagnostic;
use thiserror::Error;
use typst::diag::SourceDiagnostic;
use typst::ecow::EcoVec;
use typst::syntax::DiagSpanKind;

use crate::error::Annotated;

#[derive(Error, Debug)]
#[error("{kind}")]
pub struct ContentError {
    kind: ContentErrorKind,
}

impl ContentError {
    pub fn frontmatter_eval(
        path: &std::path::Path,
        src: &str,
        errs: EcoVec<SourceDiagnostic>,
    ) -> Self {
        let diags: Vec<_> = errs
            .into_iter()
            .map(|e| FrontmatterDiag::new(src, e))
            .collect();
        Self {
            kind: ContentErrorKind::FrontmatterEval {
                path: path.display().to_string(),
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

    /// A known frontmatter key whose value has the wrong type — previously
    /// dropped silently (`title: 3` vanished, `draft: "yes"` became `false`).
    pub fn frontmatter_field(
        path: &std::path::Path,
        key: &str,
        expected: &'static str,
        got: &'static str,
        help: Option<&'static str>,
    ) -> Self {
        Self {
            kind: ContentErrorKind::FrontmatterField {
                path: path.display().to_string(),
                key: key.to_owned(),
                expected,
                got: got.to_owned(),
                help: help.map(str::to_owned),
            },
        }
    }

    /// A frontmatter key that is a near-miss of a known one (a typo). Unknown
    /// keys with no close match pass through to `extra` untouched.
    pub fn unknown_frontmatter(path: &std::path::Path, key: &str, suggestion: &str) -> Self {
        Self {
            kind: ContentErrorKind::UnknownFrontmatterKey {
                path: path.display().to_string(),
                key: key.to_owned(),
                help: format!("did you mean `{suggestion}`?"),
            },
        }
    }

    /// A name (filename stem, frontmatter slug, or taxonomy term) with no
    /// URL-safe characters.
    pub fn empty_slug(name: &str) -> Self {
        Self {
            kind: ContentErrorKind::EmptySlug { name: name.to_owned() },
        }
    }

    /// Two pages resolving to the same permalink (a silent overwrite otherwise).
    pub fn collision(permalink: &str, first: &str, second: &str) -> Self {
        Self {
            kind: ContentErrorKind::Collision {
                permalink: permalink.to_owned(),
                first: first.to_owned(),
                second: second.to_owned(),
            },
        }
    }

    /// Two taxonomy terms slugging to the same URL.
    pub fn term_collision(taxonomy: &str, slug: &str, first: &str, second: &str) -> Self {
        Self {
            kind: ContentErrorKind::TermCollision {
                taxonomy: taxonomy.to_owned(),
                slug: slug.to_owned(),
                first: first.to_owned(),
                second: second.to_owned(),
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
    #[error("failed to evaluate frontmatter in {path}")]
    #[diagnostic(
        code(baudelaire::content::frontmatter_eval),
        help("frontmatter must be a typst dict literal of plain data")
    )]
    FrontmatterEval {
        path: String,
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

    #[error("frontmatter `{key}` in {path} must be {expected}, but is a {got}")]
    #[diagnostic(code(baudelaire::content::frontmatter_field))]
    FrontmatterField {
        path: String,
        key: String,
        expected: &'static str,
        got: String,
        #[help]
        help: Option<String>,
    },

    #[error("unknown frontmatter key `{key}` in {path}")]
    #[diagnostic(code(baudelaire::content::unknown_frontmatter_key))]
    UnknownFrontmatterKey {
        path: String,
        key: String,
        #[help]
        help: String,
    },

    #[error("`{name}` has no URL-safe characters, so its slug would be empty")]
    #[diagnostic(
        code(baudelaire::content::empty_slug),
        help("give it a `slug` with at least one ASCII letter or digit")
    )]
    EmptySlug { name: String },

    #[error("`{first}` and `{second}` both resolve to `{permalink}`")]
    #[diagnostic(
        code(baudelaire::content::collision),
        help("two pages cannot share a URL — rename one, or set a distinct `slug`/`permalink`")
    )]
    Collision {
        permalink: String,
        first: String,
        second: String,
    },

    #[error("terms `{first}` and `{second}` of `{taxonomy}` both slug to `{slug}`")]
    #[diagnostic(
        code(baudelaire::content::term_collision),
        help("two terms cannot share a URL — rename one so their slugs differ")
    )]
    TermCollision {
        taxonomy: String,
        slug: String,
        first: String,
        second: String,
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

    fn labels(&self) -> Option<Box<dyn Iterator<Item = miette::LabeledSpan> + '_>> {
        // Frontmatter is evaluated with `SpanMode::Mapped`, so each diagnostic
        // carries a raw byte range into `src` (the snippet shown above) — draw
        // an underline there instead of the whole snippet.
        let DiagSpanKind::Range { range, .. } = self.inner.span.get() else {
            return None;
        };
        let label = miette::LabeledSpan::new(
            Some(self.inner.message.to_string()),
            range.start,
            range.len(),
        );
        Some(Box::new(std::iter::once(label)))
    }
}

impl From<ContentError> for crate::error::BaudelaireErrorKind {
    fn from(e: ContentError) -> Self {
        Self::Content(Box::new(e))
    }
}
