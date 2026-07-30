//! Inlining an SVG file into the page DOM.

use miette::Diagnostic;
use thiserror::Error;

use crate::ui::{Code, Text};

/// An SVG that `svg()` asked for but baudelaire could not turn into DOM nodes.
///
/// Fatal rather than a warning: the element is already in the page, so a
/// failure here would ship an empty `<svg>` where an icon was asked for, and
/// nothing downstream would notice.
#[derive(Debug, Error, Diagnostic)]
pub enum SvgError {
    #[error("{} is not a path inside the project", Code(.path))]
    #[diagnostic(
        code(baudelaire::svg::path),
        help(
            "`svg()` takes a path from the project root, starting with `/` and \
             without `..`: the file is read by the build, not by typst, so it \
             cannot resolve relative to the calling template"
        )
    )]
    Path { path: String },

    #[error("{} is not an SVG: its root element is `<{}>`", Code(.path), Text(.root))]
    #[diagnostic(
        code(baudelaire::svg::root),
        help("`svg()` inlines an SVG file; the root element must be `<svg>`")
    )]
    Root { path: String, root: String },

    #[error("{} carries {what}, which would run on every page that inlines it", Code(.path))]
    #[diagnostic(
        code(baudelaire::svg::active),
        help(
            "an inlined SVG becomes part of the page, so its scripts execute; \
             strip it from the file, or put the script in your template where \
             it is visible"
        )
    )]
    Active { path: String, what: String },

    #[error("{} could not be read", Code(.path))]
    #[diagnostic(code(baudelaire::svg::unreadable))]
    Unreadable {
        path: String,
        /// The read failure, kept whole rather than stringified: it is
        /// baudelaire's own typed diagnostic and already carries the resolved
        /// path and the OS reason. Boxed only to break the type cycle through
        /// [`crate::error::BaudelaireErrorKind`].
        #[source]
        source: Box<crate::error::BaudelaireErrorKind>,
    },

    #[error("{} nests more than {limit} levels deep", Code(.path))]
    #[diagnostic(
        code(baudelaire::svg::nested),
        help("an icon is a few levels of `<g>`; this looks generated, not drawn")
    )]
    Nested { path: String, limit: usize },

    #[error("{} is not well-formed XML: {}", Code(.path), Text(.why))]
    #[diagnostic(
        code(baudelaire::svg::malformed),
        help("an SVG must parse as XML: every tag closed, every attribute quoted")
    )]
    Malformed { path: String, why: String },

    #[error(
        "{} contains a `<{}>` that cannot be written as HTML: {}",
        Code(.path),
        Text(.tag),
        Text(.why)
    )]
    #[diagnostic(
        code(baudelaire::svg::tag),
        help(
            "typst's HTML writer rejects a few hyphenated SVG tag names \
             (`font-face` and friends); remove the element, or reference the \
             file with `image()` instead of inlining it"
        )
    )]
    Tag {
        path: String,
        tag: String,
        why: String,
    },
}

impl SvgError {
    pub fn path(path: &str) -> Self {
        Self::Path {
            path: path.to_owned(),
        }
    }

    pub fn root(path: &str, root: &str) -> Self {
        Self::Root {
            path: path.to_owned(),
            root: root.to_owned(),
        }
    }

    /// `what` names the offending construct and is already marked up (see
    /// [`crate::ui::markup!`]): it is written at the call site, not read off the
    /// file, so it is interpolated as-is rather than escaped.
    pub fn active(path: &str, what: impl std::fmt::Display) -> Self {
        Self::Active {
            path: path.to_owned(),
            what: what.to_string(),
        }
    }

    pub fn unreadable(path: &str, source: crate::error::BaudelaireErrorKind) -> Self {
        Self::Unreadable {
            path: path.to_owned(),
            source: Box::new(source),
        }
    }

    pub fn nested(path: &str, limit: usize) -> Self {
        Self::Nested {
            path: path.to_owned(),
            limit,
        }
    }

    /// The XML parser's own message, kept as text: `roxmltree::Error` already
    /// carries the position, and naming the type here would put a parsing crate
    /// in the error API for one string.
    pub fn malformed(path: &str, why: impl std::fmt::Display) -> Self {
        Self::Malformed {
            path: path.to_owned(),
            why: why.to_string(),
        }
    }

    pub fn tag(path: &str, tag: &str, why: impl std::fmt::Display) -> Self {
        Self::Tag {
            path: path.to_owned(),
            tag: tag.to_owned(),
            why: why.to_string(),
        }
    }
}
