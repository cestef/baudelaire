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

pub type Result<T, E = BaudelaireErrorKind> = std::result::Result<T, E>;

#[derive(thiserror::Error, miette::Diagnostic, Debug)]
pub enum BaudelaireErrorKind {
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
