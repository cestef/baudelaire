//! Social card rendering failures.

use miette::Diagnostic;
use thiserror::Error;

/// A card that could not be produced. Fatal: the page's `og:image` already
/// names it, so a missing card is a broken social preview, not a cosmetic loss.
#[derive(Debug, Error, Diagnostic)]
pub enum CardError {
    #[error("the card for `{page}` rendered {pages} pages")]
    #[diagnostic(
        code(baudelaire::cards::overflow),
        help(
            "a card is one image: keep the template's content within the configured \
             `width`/`height`, or raise them"
        )
    )]
    Pages { page: String, pages: usize },

    #[error("the card for `{page}` could not be encoded: {why}")]
    #[diagnostic(code(baudelaire::cards::encode))]
    Encode { page: String, why: String },
}

impl CardError {
    pub fn pages(page: &str, pages: usize) -> Self {
        Self::Pages {
            page: page.to_owned(),
            pages,
        }
    }

    /// The encoder's own message, kept as text: its error type belongs to a
    /// crate reached only through the rasterizer, and naming it here would make
    /// a transitive dependency a direct one for one string.
    pub fn encode(page: &str, why: impl std::fmt::Display) -> Self {
        Self::Encode {
            page: page.to_owned(),
            why: why.to_string(),
        }
    }
}
