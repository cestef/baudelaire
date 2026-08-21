//! `security { }`: what the built pages tell a browser to trust.

pub mod csp;

use dispatch_derive::Table;

use crate::config::CspConfig;
use crate::config::dispatch::{Block, Section};
use crate::config::vocab::rule;

/// What the built pages tell a browser to trust: the integrity of the files
/// they load, and the policy they are served under.
#[derive(Debug, Clone, Default, Hash, Table)]
pub struct SecurityConfig {
    /// Stamp `integrity` onto every emitted script and stylesheet. Needs `assets { fingerprint }`.
    ///
    /// A digest pinned to a URL whose contents can change under it is a page
    /// that blocks its own stylesheet.
    #[key(flag)]
    pub sri: bool,

    /// The content security policy written into `_headers`. Its presence turns it on; `#false` turns it off again.
    #[key(nested(CspConfig))]
    pub csp: CspConfig,
}
