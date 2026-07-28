//! Turning one page into its artifacts.
//!
//! [`prepare`] decides what goes into the synthetic Typst module that binds a
//! page to its template and [`layout`] renders it, [`card`] compiles the same
//! page a second time as a paged document to draw its social image, and
//! [`image`] copies the images the render pass lifted out of the DOM into the
//! asset tree.
//!
//! Everything here is per-page and runs inside the parallel compile pool, in
//! contrast to [`super::emit`], which runs once over the finished site.

#[cfg(feature = "cards")]
pub(super) mod card;
pub(super) mod image;
pub(super) mod layout;
pub(super) mod prepare;
