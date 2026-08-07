//! Lifting an image out of the page DOM and into the asset tree.

use miette::Diagnostic;
use thiserror::Error;

use crate::ui::Code;

/// An image marker the render pass refused to resolve.
///
/// Fatal rather than a warning, for the same reason [`crate::error::SvgError`]
/// is: the element is already in the page, so continuing would ship a `<src>`
/// naming a file the build declined to write.
#[derive(Debug, Error, Diagnostic)]
pub enum ImageError {
    #[error("{} is not a path inside the project", Code(.path))]
    #[diagnostic(
        code(baudelaire::image::escaping),
        help(
            "the image marker is written by baudelaire's own show rule, from a \
             path typst has already resolved inside the project, so it never \
             carries `..` or a root: this one was written by hand, and the file \
             it names is read and copied by the build rather than by typst"
        )
    )]
    Escaping { path: String },
}

impl ImageError {
    /// A marker naming something other than a file under the project root.
    pub fn escaping(path: &str) -> Self {
        Self::Escaping {
            path: path.to_owned(),
        }
    }
}
