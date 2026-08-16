//! A generic AT Protocol (atproto) XRPC client: the `com.atproto.*` repository
//! methods and atproto's identifiers, with no knowledge of any lexicon.

pub mod client;
pub mod id;

pub use client::{Blob, Repo, Session};
pub use id::{AtUri, Did, Nsid, Rkey};
