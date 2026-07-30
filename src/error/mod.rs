//! Every error the crate can fail with, and the one enum that carries them.
//!
//! One module per error class, each a typed [`miette::Diagnostic`] with a
//! `baudelaire::..` code, its own fields, and a `help` that says what to do
//! about it. What was being done is a typed label ([`fs::Op`],
//! [`serialize::Artifact`], [`deploy::Step`]) rather than a message the call
//! site spells out. A foreign error is never flattened into a message or folded
//! into a neighbouring variant to save writing one: it is kept as a `#[source]`
//! under a variant of its own. [`fs`] is the model to copy.
//!
//! [`warning`] holds the same thing at `severity(warning)`: diagnostics that
//! report without failing the run.

use typst::syntax::VirtualizeError;

use crate::ui::Code;

pub mod annotated;
#[cfg(feature = "announce")]
pub mod announce;
pub mod asset;
pub mod card;
pub mod config;
pub mod content;
pub mod deploy;
pub mod fs;
pub mod hook;
pub mod link;
pub mod output;
pub mod remote;
pub mod scaffold;
pub mod serialize;
pub mod serve;
pub mod svg;
pub mod theme;
pub mod typ;
pub mod warning;

pub use annotated::Annotated;
#[cfg(feature = "announce")]
pub use announce::AnnounceError;
pub use asset::AssetError;
pub use card::CardError;
pub use config::{ConfigError, ConfigErrorKind};
pub use content::ContentError;
pub use deploy::DeployError;
pub use fs::{FsError, Op};
pub use hook::{HookError, Phase as HookPhase};
pub use link::{Broken, BrokenLinks, Dead, DeadLinks};
pub use output::BaseUrlRequired;
pub use remote::RemoteError;
pub use scaffold::ScaffoldError;
pub use serialize::{Artifact, SerializeError};
pub use serve::ServeError;
pub use svg::SvgError;
pub use theme::ThemeError;
pub use typ::TypstSourceDiagnostic;

pub type Result<T, E = BaudelaireErrorKind> = std::result::Result<T, E>;

#[derive(thiserror::Error, miette::Diagnostic, Debug)]
pub enum BaudelaireErrorKind {
    #[error(transparent)]
    #[diagnostic(code(baudelaire::typst::virtualize))]
    Virtualize(#[from] VirtualizeError),

    /// The terminal itself failed, reading or writing. The one remaining
    /// implicit `io::Error` conversion: every filesystem operation goes through
    /// [`crate::fs`] and carries path + operation context as [`FsError`], so
    /// the only bare `io::Error`s left are the two ends of the terminal,
    /// [`crate::cli::prompt`] drawing a prompt and reading the answer back, and
    /// [`crate::remote::Options`] reading a piped secret from stdin. ([`crate::ui`]
    /// itself never reaches here: its writes are infallible by design.)
    ///
    /// Still a blanket conversion, and so still a finding: each of those is its
    /// own error class and wants its own variant, with the `#[from]` dropped
    /// once the last `?` on an `io::Error` is gone. Until then the message at
    /// least names what actually failed, rather than calling a failed stdin
    /// read a failed write.
    #[error("terminal I/O failed")]
    #[diagnostic(code(baudelaire::terminal))]
    Terminal(#[from] std::io::Error),

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
    DeadLinks(#[from] crate::error::link::DeadLinks),

    #[error(transparent)]
    #[diagnostic(transparent)]
    Card(#[from] crate::error::card::CardError),

    #[error(transparent)]
    #[diagnostic(transparent)]
    Theme(#[from] crate::error::theme::ThemeError),

    #[error(transparent)]
    #[diagnostic(transparent)]
    Svg(#[from] crate::error::svg::SvgError),

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
    Generated(#[from] crate::error::BaseUrlRequired),

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
    #[cfg(feature = "announce")]
    Announce(#[from] crate::error::AnnounceError),

    #[error(transparent)]
    #[diagnostic(transparent)]
    Remote(#[from] crate::error::RemoteError),

    #[error(transparent)]
    #[diagnostic(transparent)]
    Deploy(#[from] crate::error::DeployError),

    #[error(transparent)]
    #[diagnostic(transparent)]
    Build(#[from] BuildFailed),

    #[error(transparent)]
    #[diagnostic(transparent)]
    FeedDate(#[from] FeedDateError),

    #[error(transparent)]
    #[diagnostic(transparent)]
    Strict(#[from] StrictWarnings),

    #[error(transparent)]
    #[diagnostic(transparent)]
    Unattended(#[from] Unattended),
}

/// A mutating action needed confirming, with no terminal to confirm at.
///
/// Reached only once the action is known to need an answer: `--yes` and
/// `--dry-run` are both settled before anything asks. Answering on the user's
/// behalf instead is what let a CI run that forgot `--yes` skip every deploy
/// backend and exit 0 having published nothing.
///
/// Not remote-specific, which is why it does not live on [`RemoteError`]:
/// `clean` sweeps the output directory and every scrap of local build state,
/// and wants the same answer to the same question.
#[derive(thiserror::Error, miette::Diagnostic, Debug)]
#[error("cannot confirm {} without a terminal", crate::ui::Text(.action))]
#[diagnostic(
    code(baudelaire::confirm::unattended),
    help("pass `--yes` to confirm non-interactively, or `--dry-run` to preview")
)]
pub struct Unattended {
    pub action: String,
}

/// A run that warned, under `--strict`.
///
/// Warnings are what baudelaire says instead of failing: a missing font, an
/// untaken permalink, a capability this binary lacks. Every one is a thing the
/// author probably wants to know and possibly wants to block on, and until
/// `--strict` existed the only warning CI could gate on was broken links
/// (`--strict-links`). Grepping stderr for the rest is not a gate.
#[derive(thiserror::Error, miette::Diagnostic, Debug)]
#[error("{count} warning{} in a strict run", if *.count == 1 { "" } else { "s" })]
#[diagnostic(
    code(baudelaire::strict::warnings),
    help("the warnings are above; fix them, or drop `--strict` to let them pass")
)]
pub struct StrictWarnings {
    pub count: usize,
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

/// A page date that a feed's mandated timestamp format cannot represent:
/// RFC 2822 (RSS `pubDate`) only covers years 1900–9999, RFC 3339 (Atom
/// `updated`) years 0–9999.
#[derive(thiserror::Error, miette::Diagnostic, Debug)]
#[error("date {} of {} cannot be formatted as {standard}", Code(.date), Code(.page))]
#[diagnostic(
    code(baudelaire::feed::date),
    help(
        "RFC 2822 (RSS) covers years 1900–9999 and RFC 3339 (Atom) years 0–9999: adjust the page's `date` or drop the feed format"
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
