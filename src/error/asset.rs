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

    #[cfg(feature = "images")]
    pub fn image(path: impl std::fmt::Display, detail: impl std::fmt::Display) -> Self {
        Self::Image {
            path: path.to_string(),
            detail: detail.to_string(),
        }
    }
}
