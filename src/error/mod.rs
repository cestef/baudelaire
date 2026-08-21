//! Every error the crate can fail with, and the one enum that carries them: one
//! module per error class, each a typed [`miette::Diagnostic`].
//!
//! [`warning`] holds the same at `severity(warning)`, which reports without
//! failing a run.

use typst::syntax::VirtualizeError;

use crate::ui::Code;

pub mod aggregate;
pub mod annotated;
#[cfg(feature = "announce")]
pub mod announce;
pub mod asset;
pub mod bundle;
pub mod card;
pub mod cli;
pub mod config;
pub mod content;
pub mod deploy;
pub mod entity;
pub mod fs;
pub mod hook;
pub mod image;
pub mod link;
pub mod lint;
#[cfg(feature = "markdown")]
pub mod markdown;
pub mod mirror;
pub mod output;
pub mod remote;
pub mod scaffold;
pub mod schema;
pub mod serialize;
pub mod serve;
pub mod svg;
pub mod template;
pub mod theme;
pub mod typ;
pub mod warning;

pub use annotated::Annotated;
#[cfg(feature = "announce")]
pub use announce::AnnounceError;
pub use asset::AssetError;
pub use bundle::BundleError;
pub use card::CardError;
pub use config::{ConfigError, ConfigErrorKind};
pub use content::ContentError;
pub use deploy::DeployError;
pub use entity::EntityError;
pub use fs::{FsError, Op};
pub use hook::{HookError, Phase as HookPhase};
pub use image::ImageError;
pub use link::{Broken, BrokenLinks, Dead, DeadLinks, Orphan, OrphanPages};
pub use lint::{Flaw, Flaws, Lint, Overweight, Overweights, Sources};
pub use mirror::MirrorError;
pub use output::BaseUrlRequired;
pub use remote::RemoteError;
pub use scaffold::ScaffoldError;
pub use schema::SchemaError;
pub use serialize::{Artifact, SerializeError};
pub use serve::ServeError;
pub use svg::SvgError;
pub use template::{TemplateMissing, TemplateOwnsRoot};
pub use theme::ThemeError;
pub use typ::TypstSourceDiagnostic;

pub type Result<T, E = BaudelaireErrorKind> = std::result::Result<T, E>;

#[derive(thiserror::Error, miette::Diagnostic, Debug)]
pub enum BaudelaireErrorKind {
    #[error(transparent)]
    #[diagnostic(code(baudelaire::typst::virtualize))]
    Virtualize(#[from] VirtualizeError),

    /// The one remaining blanket `io::Error` conversion: every filesystem
    /// operation goes through [`crate::fs`] and arrives as [`FsError`], so what
    /// reaches here is the two ends of the terminal.
    #[error("terminal I/O failed")]
    #[diagnostic(code(baudelaire::terminal))]
    Terminal(#[from] std::io::Error),

    #[error(transparent)]
    #[diagnostic(transparent)]
    Fs(#[from] crate::error::FsError),

    #[error(transparent)]
    #[diagnostic(transparent)]
    TypstFile(#[from] crate::error::typ::TypstFileError),

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
    Flaws(#[from] crate::error::lint::Flaws),

    #[error(transparent)]
    #[diagnostic(transparent)]
    Overweights(#[from] crate::error::lint::Overweights),

    #[error(transparent)]
    #[diagnostic(transparent)]
    Bundle(#[from] crate::error::bundle::BundleError),

    #[error(transparent)]
    #[diagnostic(transparent)]
    Card(#[from] crate::error::card::CardError),

    #[error(transparent)]
    #[diagnostic(transparent)]
    Template(#[from] crate::error::template::TemplateMissing),

    #[error(transparent)]
    #[diagnostic(transparent)]
    TemplateRoot(#[from] crate::error::template::TemplateOwnsRoot),

    #[error(transparent)]
    #[diagnostic(transparent)]
    Theme(#[from] crate::error::theme::ThemeError),

    #[error(transparent)]
    #[diagnostic(transparent)]
    Svg(#[from] crate::error::svg::SvgError),

    #[error(transparent)]
    #[diagnostic(transparent)]
    Image(#[from] crate::error::image::ImageError),

    #[error(transparent)]
    #[diagnostic(transparent)]
    Config(Box<crate::error::ConfigError>),

    #[error(transparent)]
    #[diagnostic(transparent)]
    Content(Box<crate::error::ContentError>),

    #[error(transparent)]
    #[diagnostic(transparent)]
    Entity(Box<crate::error::EntityError>),

    #[cfg(feature = "markdown")]
    #[error(transparent)]
    #[diagnostic(transparent)]
    Markdown(Box<crate::error::markdown::MarkdownError>),

    #[error(transparent)]
    #[diagnostic(transparent)]
    Mirror(Box<crate::error::MirrorError>),

    #[error(transparent)]
    #[diagnostic(transparent)]
    Scaffold(Box<crate::error::ScaffoldError>),

    #[error(transparent)]
    #[diagnostic(transparent)]
    Schema(Box<crate::error::SchemaError>),

    #[error(transparent)]
    #[diagnostic(transparent)]
    Cli(#[from] crate::error::cli::WriteFailed),

    #[error(transparent)]
    #[diagnostic(transparent)]
    Reported(#[from] crate::error::cli::Reported),

    #[error(transparent)]
    #[diagnostic(transparent)]
    CliKey(#[from] crate::error::cli::UnknownKey),
    #[error(transparent)]
    #[diagnostic(transparent)]
    CliUnsetKey(#[from] crate::error::cli::UnsetKey),

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

/// A page's name and text, ready for the snippet a diagnostic renders, tagged
/// with the language its extension names.
pub struct PageSource(pub String, pub String);

impl PageSource {
    /// Anything unrecognized is left untagged rather than guessed, since a
    /// wrong tag highlights the snippet as the wrong language.
    const LANGUAGES: &'static [(&'static str, &'static str)] = &[
        (crate::config::Config::TYPST, "Typst"),
        (crate::config::Config::MARKDOWN, "Markdown"),
    ];

    fn language(name: &str) -> Option<&'static str> {
        let extension = std::path::Path::new(name).extension()?.to_str()?;
        Self::LANGUAGES
            .iter()
            .find(|(known, _)| *known == extension)
            .map(|(_, language)| *language)
    }
}

impl From<PageSource> for miette::NamedSource<String> {
    fn from(PageSource(name, text): PageSource) -> Self {
        let language = PageSource::language(&name);
        let named = Self::new(name, text);
        match language {
            Some(language) => named.with_language(language),
            None => named,
        }
    }
}

/// Reached only once the action is known to need an answer: `--yes` and
/// `--dry-run` are both settled before anything asks.
#[derive(thiserror::Error, miette::Diagnostic, Debug)]
#[error("cannot confirm {} without a terminal", crate::ui::Text(.action))]
#[diagnostic(
    code(baudelaire::confirm::unattended),
    help("pass `--yes` to confirm non-interactively, or `--dry-run` to preview")
)]
pub struct Unattended {
    pub action: String,
}

#[derive(thiserror::Error, miette::Diagnostic, Debug)]
#[error("{count} warning{} in a strict run", if *.count == 1 { "" } else { "s" })]
#[diagnostic(
    code(baudelaire::strict::warnings),
    help("the warnings are above; fix them, or drop `--strict` to let them pass")
)]
pub struct StrictWarnings {
    pub count: usize,
}

#[derive(thiserror::Error, miette::Diagnostic, Debug)]
#[error("{} pages failed to compile", errors.len())]
#[diagnostic(code(baudelaire::build::failed))]
pub struct BuildFailed {
    #[related]
    errors: Vec<BaudelaireErrorKind>,
}

impl BuildFailed {
    /// A single failure propagates unchanged, several aggregate under one
    /// [`BuildFailed`]; `None` when nothing failed.
    pub fn aggregate(errors: Vec<BaudelaireErrorKind>) -> Option<BaudelaireErrorKind> {
        match errors.len() {
            0 => None,
            1 => errors.into_iter().next(),
            _ => Some(Self { errors }.into()),
        }
    }
}

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

impl BaudelaireErrorKind {
    /// Names the file a config diagnostic points into, for the config texts
    /// that are not `config.kdl`; everything else passes through untouched.
    pub fn named(self, path: &std::path::Path) -> Self {
        match self {
            Self::Config(error) => Self::Config(Box::new(error.named(path))),
            other => other,
        }
    }

    /// Whether the command has already reported this failure in its own format,
    /// so nothing further should be rendered for it.
    pub fn reported(&self) -> bool {
        matches!(self, Self::Reported(_))
    }
}

impl From<crate::error::ConfigError> for BaudelaireErrorKind {
    fn from(e: crate::error::ConfigError) -> Self {
        Self::Config(Box::new(e))
    }
}
