//! Asset-pipeline errors, each naming which asset failed and at which step,
//! with the underlying tool's own message as the hint.

use miette::Diagnostic;
use thiserror::Error;

// Every variant is gated, so the `slim` flavor has no message to build; `sass`
// is not named because it enables `css`.
#[cfg(any(
    feature = "css",
    feature = "js",
    feature = "images",
    feature = "tailwind"
))]
use crate::ui::{Code, Text};

#[derive(Debug, Error, Diagnostic)]
pub enum AssetError {
    #[cfg(feature = "css")]
    #[error("failed to minify CSS asset {}", Code(.path))]
    #[diagnostic(code(baudelaire::asset::css))]
    Css {
        path: String,
        #[help]
        detail: Text<String>,
    },

    #[cfg(feature = "sass")]
    #[error("failed to compile Sass asset {}", Code(.path))]
    #[diagnostic(code(baudelaire::asset::sass))]
    Sass {
        path: String,
        #[source]
        source: Box<grass::Error>,
    },

    /// The generator itself cannot fail: a class name it does not know is not
    /// one, and it writes no rule for it.
    #[cfg(feature = "tailwind")]
    #[error("failed to read the utility stylesheet config {}", Code(.path))]
    #[diagnostic(code(baudelaire::asset::tailwind))]
    Tailwind {
        path: String,
        #[source]
        source: encre_css::Error,
    },

    #[cfg(feature = "js")]
    #[error("failed to bundle JavaScript asset {}", Code(.path))]
    #[diagnostic(code(baudelaire::asset::js))]
    Js {
        path: String,
        #[help]
        detail: Text<String>,
    },

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

    #[cfg(feature = "js")]
    #[error("no TypeScript config at {}", Code(.path))]
    #[diagnostic(
        code(baudelaire::asset::tsconfig),
        help(
            "`assets {{ tsconfig }}` is a path relative to the project root; drop it to let the bundler find one per script"
        )
    )]
    Tsconfig { path: String },

    #[cfg(feature = "images")]
    #[error("failed to optimize image asset {}", Code(.path))]
    #[diagnostic(code(baudelaire::asset::image))]
    Image {
        path: String,
        #[help]
        detail: Text<String>,
    },
}

impl AssetError {
    #[cfg(feature = "css")]
    pub fn css(path: impl std::fmt::Display, detail: impl std::fmt::Display) -> Self {
        Self::Css {
            path: path.to_string(),
            detail: Text(detail.to_string()),
        }
    }

    #[cfg(feature = "sass")]
    pub fn sass(path: impl std::fmt::Display, source: Box<grass::Error>) -> Self {
        Self::Sass {
            path: path.to_string(),
            source,
        }
    }

    #[cfg(feature = "tailwind")]
    pub fn tailwind(path: impl std::fmt::Display, source: encre_css::Error) -> Self {
        Self::Tailwind {
            path: path.to_string(),
            source,
        }
    }

    #[cfg(feature = "js")]
    pub fn js(path: impl std::fmt::Display, detail: impl std::fmt::Display) -> Self {
        Self::Js {
            path: path.to_string(),
            detail: Text(detail.to_string()),
        }
    }

    #[cfg(feature = "js")]
    pub fn tsconfig(path: impl std::fmt::Display) -> Self {
        Self::Tsconfig {
            path: path.to_string(),
        }
    }

    #[cfg(feature = "js")]
    pub fn runtime(source: std::io::Error) -> Self {
        Self::Runtime { source }
    }

    #[cfg(feature = "images")]
    pub fn image(path: impl std::fmt::Display, detail: impl std::fmt::Display) -> Self {
        Self::Image {
            path: path.to_string(),
            detail: Text(detail.to_string()),
        }
    }
}
