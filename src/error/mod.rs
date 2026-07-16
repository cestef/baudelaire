use typst::syntax::VirtualizeError;

pub mod annotated;
pub mod asset;
pub mod config;
pub mod content;
pub mod fs;
pub mod hook;
pub mod link;
pub mod announce;
pub mod scaffold;
pub mod serialize;
pub mod serve;
pub mod typ;
pub mod warning;

pub use annotated::Annotated;
pub use asset::AssetError;
pub use config::{ConfigError, ConfigErrorKind};
pub use content::ContentError;
pub use fs::{FsError, Op};
pub use hook::{HookError, Phase as HookPhase};
pub use link::{Broken, BrokenLinks};
pub use announce::AnnounceError;
pub use scaffold::ScaffoldError;
pub use serialize::{Artifact, SerializeError};
pub use serve::ServeError;
pub use typ::TypstSourceDiagnostic;

pub type Result<T, E = BaudelaireErrorKind> = std::result::Result<T, E>;

#[derive(thiserror::Error, miette::Diagnostic, Debug)]
pub enum BaudelaireErrorKind {
    #[error(transparent)]
    #[diagnostic(code(baudelaire::typst::virtualize))]
    Virtualize(#[from] VirtualizeError),

    /// A terminal write failed while reporting progress. The one remaining
    /// implicit `io::Error` conversion: every filesystem operation goes through
    /// [`crate::fs`] and carries path + operation context as [`FsError`], so
    /// only [`crate::cli::output::Report`] (which writes to stdout) produces
    /// bare `io::Error`s.
    #[error("failed to write CLI output")]
    #[diagnostic(code(baudelaire::output))]
    Output(#[from] std::io::Error),

    #[error(transparent)]
    #[diagnostic(transparent)]
    Fs(#[from] crate::error::FsError),

    #[error("typst compilation failed")]
    #[diagnostic(code(baudelaire::typst::compile))]
    TypstCompile(#[related] Vec<TypstSourceDiagnostic>),

    #[error("typst html rendering failed")]
    #[diagnostic(code(baudelaire::typst::html))]
    TypstHtml(#[related] Vec<TypstSourceDiagnostic>),

    #[error(transparent)]
    #[diagnostic(transparent)]
    BrokenLinks(#[from] crate::error::link::BrokenLinks),

    #[error(transparent)]
    #[diagnostic(transparent)]
    Config(Box<crate::error::ConfigError>),

    #[error(transparent)]
    #[diagnostic(transparent)]
    Content(Box<crate::error::ContentError>),

    #[error(transparent)]
    #[diagnostic(transparent)]
    Scaffold(Box<crate::error::ScaffoldError>),

    #[error(transparent)]
    #[diagnostic(transparent)]
    Serve(Box<crate::error::serve::ServeError>),

    #[error(transparent)]
    #[diagnostic(transparent)]
    Serialize(#[from] crate::error::SerializeError),

    #[error(transparent)]
    #[diagnostic(transparent)]
    Asset(#[from] crate::error::AssetError),

    #[error(transparent)]
    #[diagnostic(transparent)]
    Hook(#[from] crate::error::HookError),

    #[error(transparent)]
    #[diagnostic(transparent)]
    Announce(#[from] crate::error::AnnounceError),

    #[error(transparent)]
    #[diagnostic(transparent)]
    Build(#[from] BuildFailed),

    #[error(transparent)]
    #[diagnostic(transparent)]
    FeedDate(#[from] FeedDateError),
}

/// Several pages failed to compile in one build. Each page's own diagnostics
/// are attached as related errors, so a build with three broken pages renders
/// all three instead of only the first.
#[derive(thiserror::Error, miette::Diagnostic, Debug)]
#[error("{} pages failed to compile", errors.len())]
#[diagnostic(code(baudelaire::build::failed))]
pub struct BuildFailed {
    #[related]
    errors: Vec<BaudelaireErrorKind>,
}

impl BuildFailed {
    /// Collapse per-page failures into one error: a single failure propagates
    /// unchanged (its diagnostic is already precise), several aggregate under
    /// one [`BuildFailed`]. `None` when nothing failed.
    pub fn aggregate(errors: Vec<BaudelaireErrorKind>) -> Option<BaudelaireErrorKind> {
        match errors.len() {
            0 => None,
            1 => errors.into_iter().next(),
            _ => Some(Self { errors }.into()),
        }
    }
}

/// A page date that a feed's mandated timestamp format cannot represent —
/// RFC 2822 (RSS `pubDate`) only covers years 1900–9999, RFC 3339 (Atom
/// `updated`) years 0–9999.
#[derive(thiserror::Error, miette::Diagnostic, Debug)]
#[error("date `{date}` of `{page}` cannot be formatted as {standard}")]
#[diagnostic(
    code(baudelaire::feed::date),
    help(
        "RFC 2822 (RSS) covers years 1900–9999 and RFC 3339 (Atom) years 0–9999 — adjust the page's `date` or drop the feed format"
    )
)]
pub struct FeedDateError {
    page: String,
    date: String,
    standard: &'static str,
    #[source]
    source: time::error::Format,
}

impl FeedDateError {
    pub fn new(
        page: impl Into<String>,
        date: impl Into<String>,
        standard: &'static str,
        source: time::error::Format,
    ) -> Self {
        Self {
            page: page.into(),
            date: date.into(),
            standard,
            source,
        }
    }
}

impl From<crate::error::ConfigError> for BaudelaireErrorKind {
    fn from(e: crate::error::ConfigError) -> Self {
        Self::Config(Box::new(e))
    }
}
