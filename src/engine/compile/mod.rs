//! Turning one page into its artifacts: the synthetic module that binds it to
//! its template, the compiles that typeset it, and the files written beside its
//! HTML.

#[cfg(any(feature = "pdf", feature = "epub"))]
pub(super) mod bundle;
#[cfg(feature = "cards")]
pub(super) mod card;
pub(super) mod image;
pub(super) mod layout;
#[cfg(feature = "sidecars")]
pub(super) mod paged;
#[cfg(feature = "pdf")]
pub(super) mod pdf;
pub(super) mod prepare;
pub(super) mod sidecar;
