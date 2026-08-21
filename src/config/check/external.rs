//! `check { external { } }`: how outbound links are verified over the network.

use std::time::Duration;

use dispatch_derive::Table;

use crate::config::dispatch::Kind::Number;
use crate::config::dispatch::{Block, Section};
use crate::config::node::NodeExt;
use crate::config::value::ValueExt;
use crate::config::vocab::rule;

/// The outbound link check: whether it runs, and the manners it runs with.
#[derive(Debug, Clone, Hash, Table)]
#[table(hook(switch = enabled))]
pub struct ExternalConfig {
    /// Verify outbound `http(s)` links over the network at all. Read by `check`
    /// alone, so a build stays offline and deterministic.
    pub enabled: bool,

    /// How long a link that answered is trusted before it is asked again.
    ///
    /// Only successes are remembered.
    #[key(time)]
    pub fresh: Duration,

    /// How long one request may take before the link counts as unreachable.
    #[key(time)]
    pub timeout: Duration,

    /// How many links are fetched at once. Unset, as many as the build has threads.
    #[key(custom(
        Number,
        |c: &Self| c.concurrency.into(),
        |c: &mut Self, n: &kdl::KdlNode, t: &str| {
            let at_once: u16 = n.arg(t, 0)?.bounded(t, NodeExt::span(n), 1, u16::MAX)?;
            c.concurrency = Some(usize::from(at_once));
            Ok(())
        },
    ))]
    pub concurrency: Option<usize>,

    /// Globs, matched against each URL without its scheme, that are never requested: `ignore "*.internal/**"`.
    ///
    /// The same glob grammar as `prune { keep }`.
    #[key(texts)]
    pub ignore: Vec<String>,

    /// Status codes that count as alive, beyond the 2xx and 3xx that always do.
    ///
    /// A page behind a login answers 401 and is still there.
    #[key(numbers(u16, 100, 599))]
    pub accept: Vec<u16>,
}

impl ExternalConfig {
    /// Whether `status` says the link is there, for a host that answered.
    pub fn alive(&self, status: u16) -> bool {
        (200..400).contains(&status) || self.accept.contains(&status)
    }
}

impl Default for ExternalConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            fresh: Duration::from_hours(7 * 24),
            timeout: crate::remote::Http::TIMEOUT,
            concurrency: None,
            ignore: Vec::new(),
            accept: Vec::new(),
        }
    }
}
