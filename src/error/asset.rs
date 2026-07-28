//! Precise asset-pipeline errors.
//!
//! A minifier or bundler failure names *which* asset and *what* step failed:
//! `failed to minify CSS asset `assets/app.css`` with the underlying tool's
//! message as the actionable hint, rather than being folded into a generic I/O
//! error.

use miette::Diagnostic;
use thiserror::Error;

/// A failure while processing a static asset (minify or bundle).
#[derive(Debug, Error, Diagnostic)]
pub enum AssetError {
    /// lightningcss could not parse or print the stylesheet.
    #[cfg(feature = "css")]
    #[error("failed to minify CSS asset `{path}`")]
    #[diagnostic(code(baudelaire::asset::css))]
    Css {
        path: String,
        #[help]
        detail: String,
    },

    /// rolldown could not bundle the JavaScript entry.
    #[cfg(feature = "js")]
    #[error("failed to bundle JavaScript asset `{path}`")]
    #[diagnostic(code(baudelaire::asset::js))]
    Js {
        path: String,
        #[help]
        detail: String,
    },

    /// The bundler's async runtime could not be started, so no entry was ever
    /// reached: its own class, since there is no asset to name and the cause is
    /// an `io::Error` worth keeping whole.
    #[cfg(feature = "js")]
    #[error("failed to start the JavaScript bundler")]
    #[diagnostic(
        code(baudelaire::asset::runtime),
        help(
            "the bundler starts worker threads: check the process and thread limits \
             (`ulimit -u`, `RLIMIT_NPROC`), or turn bundling off in the `assets` config"
        )
    )]
    Runtime {
        #[source]
        source: std::io::Error,
    },

    /// oxipng could not optimize the PNG.
    #[cfg(feature = "images")]
    #[error("failed to optimize image asset `{path}`")]
    #[diagnostic(code(baudelaire::asset::image))]
    Image {
        path: String,
        #[help]
        detail: String,
    },
}

impl AssetError {
    #[cfg(feature = "css")]
    pub fn css(path: impl std::fmt::Display, detail: impl std::fmt::Display) -> Self {
        Self::Css {
            path: path.to_string(),
            detail: detail.to_string(),
        }
    }

    #[cfg(feature = "js")]
    pub fn js(path: impl std::fmt::Display, detail: impl std::fmt::Display) -> Self {
        Self::Js {
            path: path.to_string(),
            detail: detail.to_string(),
        }
    }

    /// The OS failure kept whole, rather than flattened into a hint under a
    /// made-up asset path.
    #[cfg(feature = "js")]
    pub fn runtime(source: std::io::Error) -> Self {
        Self::Runtime { source }
    }

    #[cfg(feature = "images")]
    pub fn image(path: impl std::fmt::Display, detail: impl std::fmt::Display) -> Self {
        Self::Image {
            path: path.to_string(),
            detail: detail.to_string(),
        }
    }
}
