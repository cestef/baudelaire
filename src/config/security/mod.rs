//! `security { }`: what the built pages tell a browser to trust.

pub mod csp;

use crate::config::CspConfig;
use crate::config::dispatch::Kind::Block as Nested;
use crate::config::dispatch::Kind::Flag;
use crate::config::dispatch::{Block, Section};
use crate::config::node::NodeExt;

/// What the built pages tell a browser to trust: the integrity of the files
/// they load, and the policy they are served under.
///
/// Both are derived from the pages themselves rather than written by hand,
/// which is the only way either stays true: a hand-kept `script-src` goes stale
/// the moment a template gains an inline script, and a hand-kept `integrity`
/// the moment the file it names is rebuilt.
#[derive(Debug, Clone, Default, Hash)]
pub struct SecurityConfig {
    /// Stamp `integrity` onto every script and stylesheet this build emitted,
    /// so a browser refuses one that arrives altered.
    ///
    /// Needs `assets { fingerprint }`: an attribute pinning a digest to a URL
    /// whose contents can change under it is how a site serves a page that
    /// blocks its own stylesheet.
    pub sri: bool,
    /// The `Content-Security-Policy` written into the generated `_headers`.
    pub csp: CspConfig,
}

/// The `security { .. }` section: what the pages tell a browser to trust.
impl Section for SecurityConfig {
    const RULES: Block<Self> = Block(&[
        (
            "sri",
            Flag,
            "Stamp `integrity` onto every emitted script and stylesheet. Needs `assets { fingerprint }`.",
            |c, n, t| {
                c.sri = n.boolean(t, 0)?;
                Ok(())
            },
        ),
        (
            "csp",
            Nested(CspConfig::rows),
            "The content security policy written into `_headers`. Its presence turns it on.",
            |c, n, t| c.csp.fill(n, t),
        ),
    ]);
}
