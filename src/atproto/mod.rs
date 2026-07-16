//! A minimal, generic AT Protocol (atproto) XRPC client. It speaks the
//! `com.atproto.*` repository methods and models atproto's identifiers; it knows
//! nothing about any particular lexicon. Publishers (see [`crate::announce`])
//! layer their own record shapes on top.

pub mod client;
pub mod id;

pub use client::{Blob, Repo, Session};
pub use id::{AtUri, Did, Nsid, Rkey};
