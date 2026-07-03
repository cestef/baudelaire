use miette::NamedSource;
use std::sync::OnceLock;
use typst::syntax::VirtualizeError;

pub mod annotated;
pub mod asset;
pub mod config;
pub mod content;
pub mod fs;
pub mod hook;
pub mod link;
pub mod scaffold;
pub mod serialize;
pub mod serve;
pub mod typ;

pub use annotated::Annotated;
pub use asset::AssetError;
pub use config::{ConfigError, ConfigErrorKind};
pub use content::{ContentError, ContentErrorKind};
pub use fs::{FsError, Op};
pub use hook::{HookError, Phase as HookPhase};
pub use link::{Broken, BrokenLinks};
pub use scaffold::{ScaffoldError, ScaffoldErrorKind};
pub use serialize::{Artifact, SerializeError};
pub use serve::{ServeError, ServeErrorKind};
pub use typ::TypstSourceDiagnostic;

impl From<BaudelaireErrorKind> for BaudelaireError {
    fn from(kind: BaudelaireErrorKind) -> Self {
        Self {
            kind,
            source: OnceLock::new(),
        }
    }
}

#[derive(Debug)]
pub struct BaudelaireError {
    kind: BaudelaireErrorKind,
    source: OnceLock<NamedSource<String>>,
}

impl std::fmt::Display for BaudelaireError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.kind)
    }
}

impl std::error::Error for BaudelaireError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.kind.source()
    }
}

impl miette::Diagnostic for BaudelaireError {
    fn code<'a>(&'a self) -> Option<Box<dyn std::fmt::Display + 'a>> {
        self.kind.code()
    }

    fn severity(&self) -> Option<miette::Severity> {
        self.kind.severity()
    }

    fn help<'a>(&'a self) -> Option<Box<dyn std::fmt::Display + 'a>> {
        self.kind.help()
    }

    fn url<'a>(&'a self) -> Option<Box<dyn std::fmt::Display + 'a>> {
        self.kind.url()
    }

    fn source_code(&self) -> Option<&dyn miette::SourceCode> {
        self.source.get().map(|s| s as _)
    }

    fn labels(&self) -> Option<Box<dyn Iterator<Item = miette::LabeledSpan> + '_>> {
        self.kind.labels()
    }

    fn related<'a>(&'a self) -> Option<Box<dyn Iterator<Item = &'a dyn miette::Diagnostic> + 'a>> {
        self.kind.related()
    }

    fn diagnostic_source(&self) -> Option<&dyn miette::Diagnostic> {
        self.kind.diagnostic_source()
    }
}

pub type Result<T, E = BaudelaireErrorKind> = std::result::Result<T, E>;

impl BaudelaireError {
    pub fn source(&self, src: NamedSource<String>) -> Result<()> {
        self.source
            .set(src)
            .map_err(|_| BaudelaireErrorKind::SourceAlreadySet)
    }
}

#[derive(thiserror::Error, miette::Diagnostic, Debug)]
pub enum BaudelaireErrorKind {
    #[error("source has already been set on this error")]
    #[diagnostic(code(baudelaire::internal::source_already_set))]
    SourceAlreadySet,

    #[error(transparent)]
    #[diagnostic(code(baudelaire::typst::virtualize))]
    Virtualize(#[from] VirtualizeError),

    #[error(transparent)]
    #[diagnostic(code(baudelaire::std::io))]
    Io(#[from] std::io::Error),

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
}

impl From<crate::error::ConfigError> for BaudelaireErrorKind {
    fn from(e: crate::error::ConfigError) -> Self {
        Self::Config(Box::new(e))
    }
}
