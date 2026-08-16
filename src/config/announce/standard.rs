//! `announce { standard { } }`: the standard.site (AT Protocol) target.

use std::path::PathBuf;

use crate::config::dispatch::Kind::{Block as Nested, Flag, Path, Text, Url};
use crate::config::dispatch::{Block, Section};
use crate::config::node::NodeExt;

#[derive(Debug, Clone, Hash)]
pub struct StandardConfig {
    /// Account handle or DID to authenticate as, e.g. `you.bsky.social`.
    pub handle: String,
    /// Repository DID, a stable public identifier and not a secret. When set,
    /// the build emits the verification artifacts offline rather than resolving
    /// the handle.
    pub did: Option<String>,
    /// PDS/entryway host to authenticate and write records against.
    pub pds: String,
    /// Opt the publication into discovery surfaces.
    pub discover: bool,
    /// Publication icon, a path (under the project root) uploaded as a blob.
    pub icon: Option<PathBuf>,
    /// Which build-time verification artifacts to emit (requires `did`).
    pub verify: VerifyConfig,
}

/// Which standard.site domain-verification artifacts the build emits; both
/// require a configured `did`.
#[derive(Debug, Clone, Hash)]
pub struct VerifyConfig {
    /// Emit `/.well-known/site.standard.publication`, the publication `at://`
    /// URI.
    pub wellknown: bool,
    /// Inject a per-page `<link rel="site.standard.document">` into dated
    /// pages.
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

impl Section for StandardConfig {
    const RULES: Block<Self> = Block(&[
        (
            "handle",
            Text,
            "The atproto handle the site is announced under.",
            |c, n, t| {
                c.handle = n.string(t, 0)?;
                Ok(())
            },
        ),
        (
            "did",
            Text,
            "That handle's DID, if it should not be resolved at build time.",
            |c, n, t| {
                c.did = Some(n.string(t, 0)?);
                Ok(())
            },
        ),
        (
            "pds",
            Url,
            "The personal data server the record is written to.",
            |c, n, t| {
                c.pds = n.url(t, 0)?;
                Ok(())
            },
        ),
        (
            "discover",
            Flag,
            "Show the publication on standard.site's discovery surfaces.",
            |c, n, t| {
                c.discover = n.boolean(t, 0)?;
                Ok(())
            },
        ),
        (
            "icon",
            Path,
            "An icon published with the record.",
            |c, n, t| {
                c.icon = Some(n.string(t, 0)?.into());
                Ok(())
            },
        ),
        (
            "verify",
            Nested(VerifyConfig::rows),
            "Which handle-verification artifacts the build emits.",
            |c, n, t| c.verify.fill(n, t),
        ),
    ]);
}

impl Section for VerifyConfig {
    const RULES: Block<Self> = Block(&[
        (
            "wellknown",
            Flag,
            "Write `/.well-known/site.standard.publication`, naming the publication record.",
            |c, n, t| {
                c.wellknown = n.boolean(t, 0)?;
                Ok(())
            },
        ),
        (
            "links",
            Flag,
            "Add the verification links to the page head.",
            |c, n, t| {
                c.links = n.boolean(t, 0)?;
                Ok(())
            },
        ),
    ]);
}
