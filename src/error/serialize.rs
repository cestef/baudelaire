//! Serialization errors for build artifacts, naming the artifact a
//! `serde_json::Error` alone does not.

use miette::Diagnostic;
use thiserror::Error;

/// A build artifact that is serialized to JSON, named for error messages.
#[derive(Debug, Clone, Copy)]
pub enum Artifact {
    Cache,
    SearchIndex,
    AnnounceCache,
    Feed,
    WebManifest,
    Standalone,
}

impl std::fmt::Display for Artifact {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Cache => "build cache",
            Self::SearchIndex => "search index",
            Self::AnnounceCache => "publish cache",
            Self::Feed => "JSON feed",
            Self::WebManifest => "web app manifest",
            Self::Standalone => "single-file export",
        })
    }
}

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
