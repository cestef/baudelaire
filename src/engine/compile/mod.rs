//! Turning one page into its artifacts.
//!
//! [`prepare`] decides what goes into the synthetic Typst module that binds a
//! page to its template and [`layout`] renders it, [`generated`] writes the
//! site tree and page catalogue those templates import, [`card`] compiles the same
//! page a second time as a paged document to draw its social image, and
//! [`image`] copies the images the render pass lifted out of the DOM into the
//! asset tree.
//!
//! Everything here serves the compile, in contrast to [`super::emit`], which
//! runs over the finished site. [`card`] and [`image`] run per page inside the
//! parallel pool; [`prepare`] and [`generated`] derive from the whole page set
//! and so run once, before it.

#[cfg(feature = "cards")]
pub(super) mod card;
pub(super) mod generated;
pub(super) mod image;
pub(super) mod layout;
pub(super) mod prepare;
