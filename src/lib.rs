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
pub mod generated;
pub mod graph;
pub mod mime;
pub mod mirror;
pub mod owned;
pub mod remote;
pub mod render;
pub mod theme;
pub mod ui;
pub mod version;
pub mod world;

pub use error::*;

/// The crate version, the single source every surface reads.
pub const VERSION: &str = version::Version::SEMVER;
