//! Precise serialization errors for build artifacts.
//!
//! A `serde_json::Error` alone says *what* is malformed but not *which* artifact
//! failed. [`SerializeError`] names the artifact so a failure reads `failed to
//! serialize the search index` rather than being folded into a generic I/O
//! error.

use miette::Diagnostic;
use thiserror::Error;

/// A build artifact that is serialized to JSON, named for error messages.
#[derive(Debug, Clone, Copy)]
pub enum Artifact {
    /// The incremental build cache manifest.
    Cache,
    /// A generated search index.
    SearchIndex,
}

impl std::fmt::Display for Artifact {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Cache => "build cache",
            Self::SearchIndex => "search index",
        })
    }
}

/// Failure serializing a build artifact to JSON.
#[derive(Debug, Error, Diagnostic)]
#[error("failed to serialize the {artifact}")]
#[diagnostic(code(baudelaire::serialize))]
pub struct SerializeError {
    artifact: Artifact,
    #[source]
    source: serde_json::Error,
}

impl SerializeError {
    pub fn new(artifact: Artifact, source: serde_json::Error) -> Self {
        Self { artifact, source }
    }
}
