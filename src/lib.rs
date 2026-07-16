pub mod announce;
pub mod atproto;
pub mod cli;
pub mod codegen;
pub mod config;
pub mod content;
pub mod deploy;
pub mod engine;
pub mod error;
pub mod fs;
pub mod graph;
pub mod mime;
pub mod remote;
pub mod render;
pub mod ui;
pub mod world;

pub use error::*;

/// The crate version, the single source for every place that surfaces it: the
/// CLI banner, the HTTP user agent, `sys.inputs.baudelaire.version`, and the
/// `baudelaire:site` module.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
