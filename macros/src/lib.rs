//! `#[derive(Table)]`: a dispatch table assembled from a struct's fields, their
//! doc comments, and one row macro the consuming crate owns.
//!
//! This crate holds no vocabulary of its own. It decides *which* fields become
//! rows, in what order, under what key and with what documentation, then hands
//! each one to a `macro_rules!` the consumer wrote:
//!
//! ```text
//! #[derive(Table)]
//! #[table(impl = Section, const RULES: Block<Self> = Block, rule = crate::rule)]
//! struct Highlight {
//!     /// What every emitted class starts with. Empty for none.
//!     #[key(text)]
//!     prefix: String,
//! }
//! ```
//!
//! expands to
//!
//! ```text
//! impl Section for Highlight {
//!     const RULES: Block<Self> = Block(&[
//!         crate::rule!(@row Self, "prefix", prefix, "What every emitted class starts with. Empty for none.", text),
//!     ]);
//! }
//! ```
//!
//! What `text` means is the consumer's business. See `README.md`.

mod key;
mod options;
mod table;

use proc_macro::TokenStream;
use syn::{DeriveInput, parse_macro_input};

use table::Table;

/// Derives an impl carrying one associated table, one row per `#[key]` field.
#[proc_macro_derive(Table, attributes(table, key))]
pub fn table(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    match Table::try_from(input) {
        Ok(table) => table.expand().into(),
        Err(error) => error.into_compile_error().into(),
    }
}
