//! `announce { standard { } }`: the standard.site (AT Protocol) target.

use std::path::PathBuf;

use dispatch_derive::Table;

use crate::config::dispatch::{Block, Section};
use crate::config::vocab::rule;

#[derive(Debug, Clone, Hash, Table)]
pub struct StandardConfig {
    /// The atproto handle the site is announced under.
    ///
    /// A handle or DID to authenticate as, e.g. `you.bsky.social`.
    #[key(text)]
    pub handle: String,

    /// That handle's DID, if it should not be resolved at build time.
    ///
    /// A stable public identifier, not a secret. When set, the build emits the
    /// verification artifacts offline rather than resolving the handle.
    #[key(opt text)]
    pub did: Option<String>,

    /// The personal data server the record is written to.
    #[key(url)]
    pub pds: String,

    /// Show the publication on standard.site's discovery surfaces.
    #[key(flag)]
    pub discover: bool,

    /// An icon published with the record.
    ///
    /// A path under the project root, uploaded as a blob.
    #[key(opt path)]
    pub icon: Option<PathBuf>,

    /// Which handle-verification artifacts the build emits.
    ///
    /// Both require a configured `did`.
    #[key(nested(VerifyConfig))]
    pub verify: VerifyConfig,
}

/// Which standard.site domain-verification artifacts the build emits; both
/// require a configured `did`.
#[derive(Debug, Clone, Hash, Table)]
pub struct VerifyConfig {
    /// Write `/.well-known/site.standard.publication`, naming the publication record.
    #[key(flag)]
    pub wellknown: bool,

    /// Add the verification links to the page head.
    ///
    /// A per-page `<link rel="site.standard.document">` on dated pages.
    #[key(flag)]
    pub links: bool,
}

impl Default for StandardConfig {
    fn default() -> Self {
        Self {
            handle: String::new(),
            did: None,
            pds: "https://bsky.social".into(),
            discover: true,
            icon: None,
            verify: VerifyConfig::default(),
        }
    }
}

impl Default for VerifyConfig {
    fn default() -> Self {
        Self {
            wellknown: true,
            links: true,
        }
    }
}
