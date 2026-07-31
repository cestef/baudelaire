//! Turning one page into its artifacts.
//!
//! [`prepare`] decides what goes into the synthetic Typst module that binds a
//! page to its template and [`layout`] renders it, [`generated`] writes the
//! site tree and page catalogue those templates import, [`sidecar`] compiles the
//! same page a second time as a *paged* document to draw the files that sit
//! beside its HTML ([`card`] is one), and [`image`] copies the images the render
//! pass lifted out of the DOM into the asset tree.
//!
//! Everything here serves the compile, in contrast to [`super::emit`], which
//! runs over the finished site. [`sidecar`] and [`image`] run per page inside the
//! parallel pool; [`prepare`] and [`generated`] derive from the whole page set
//! and so run once, before it.

#[cfg(feature = "pdf")]
pub(super) mod bundle;
#[cfg(feature = "cards")]
pub(super) mod card;
pub(super) mod generated;
pub(super) mod image;
pub(super) mod layout;
#[cfg(feature = "sidecars")]
pub(super) mod paged;
#[cfg(feature = "pdf")]
pub(super) mod pdf;
pub(super) mod prepare;
pub(super) mod sidecar;
