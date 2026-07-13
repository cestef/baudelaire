//! Incremental build support: content hashing, dependency scanning, and an
//! authoritative on-disk cache that reuses unchanged pages.

mod cache;
mod deps;
mod digest;
mod hash;

pub use cache::{Cache, RenderInputs};
pub use deps::Deps;
pub use digest::FileDigests;
pub use hash::Hash;
