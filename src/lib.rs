#[cfg(feature = "announce")]
pub mod announce;
#[cfg(feature = "announce")]
pub mod atproto;
pub mod cli;
pub mod codegen;
pub mod config;
pub mod content;
pub mod deploy;
pub mod digest;
pub mod engine;
pub mod error;
pub mod fs;
pub mod graph;
pub mod mime;
pub mod remote;
pub mod render;
pub mod theme;
pub mod ui;
pub mod version;
pub mod world;

pub use error::*;

/// The crate version, the single source for every place that surfaces it: the
/// CLI banner, the HTTP user agent, `sys.inputs.baudelaire.version`, and the
/// `baudelaire:site` module. The provenance behind it lives on
/// [`version::Version`].
pub const VERSION: &str = version::Version::SEMVER;
