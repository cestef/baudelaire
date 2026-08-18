//! `security { }`: what the built pages tell a browser to trust.

pub mod csp;

use crate::config::CspConfig;
use crate::config::dispatch::Kind::Block as Nested;
use crate::config::dispatch::Kind::Flag;
use crate::config::dispatch::{Block, Section};
use crate::config::node::NodeExt;

/// What the built pages tell a browser to trust: the integrity of the files
/// they load, and the policy they are served under.
#[derive(Debug, Clone, Default, Hash)]
pub struct SecurityConfig {
    /// Stamp `integrity` onto every script and stylesheet this build emitted.
    /// Needs `assets { fingerprint }`: a digest pinned to a URL whose contents
    /// can change under it is a page that blocks its own stylesheet.
    pub sri: bool,
    /// The `Content-Security-Policy` written into the generated `_headers`.
    pub csp: CspConfig,
}

impl Section for SecurityConfig {
    const RULES: Block<Self> = Block(&[
        (
            "sri",
            Flag,
            "Stamp `integrity` onto every emitted script and stylesheet. Needs `assets { fingerprint }`.",
            |c| c.sri.into(),
            |c, n, t| {
                c.sri = n.boolean(t, 0)?;
                Ok(())
            },
        ),
        (
            "csp",
            Nested(CspConfig::rows),
            "The content security policy written into `_headers`. Its presence turns it on; `#false` turns it off again.",
            |c| c.csp.values(),
            |c, n, t| c.csp.fill(n, t),
        ),
    ]);
}
