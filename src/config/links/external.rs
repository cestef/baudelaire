//! `links { external { } }`: how outbound links are verified over the network.

use std::time::Duration;

use crate::config::dispatch::Kind::{Number, Numbers, Texts, Time};
use crate::config::dispatch::{Block, Section, Switch};
use crate::config::node::NodeExt;
use crate::config::value::ValueExt;

/// The outbound link check: whether it runs, and the manners it runs with.
#[derive(Debug, Clone, Hash)]
pub struct ExternalConfig {
    /// Verify outbound `http(s)` links over the network at all. Read by `check`
    /// alone, so a build stays offline and deterministic.
    pub enabled: bool,
    /// How long a URL that answered stays answered; only successes are
    /// remembered.
    pub fresh: Duration,
    /// How long one request may take before it counts as unreachable.
    pub timeout: Duration,
    /// How many requests are in flight at once. `None` leaves it to the build's
    /// own thread pool.
    pub concurrency: Option<usize>,
    /// Globs, matched against each URL without its scheme, that are never
    /// requested. The same glob grammar as `prune { keep }`: `*.internal/**`.
    pub ignore: Vec<String>,
    /// Status codes that count as the link working, beyond the 2xx and 3xx that
    /// always do: a page behind a login answers 401 and is still there.
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

impl Section for ExternalConfig {
    const SWITCH: Option<Switch<Self>> = Some(Switch {
        set: |c, on| c.enabled = on,
        on: |c| c.enabled,
    });

    const RULES: Block<Self> = Block(&[
        (
            "fresh",
            Time,
            "How long a link that answered is trusted before it is asked again.",
            |c| c.fresh.into(),
            |c, n, t| {
                c.fresh = n.duration(t, 0)?;
                Ok(())
            },
        ),
        (
            "timeout",
            Time,
            "How long one request may take before the link counts as unreachable.",
            |c| c.timeout.into(),
            |c, n, t| {
                c.timeout = n.duration(t, 0)?;
                Ok(())
            },
        ),
        (
            "concurrency",
            Number,
            "How many links are fetched at once. Unset, as many as the build has threads.",
            |c| c.concurrency.into(),
            |c, n, t| {
                let at_once: u16 = n.arg(t, 0)?.bounded(t, NodeExt::span(n), 1, u16::MAX)?;
                c.concurrency = Some(usize::from(at_once));
                Ok(())
            },
        ),
        (
            "ignore",
            Texts,
            "Globs, matched against each URL without its scheme, that are never requested: `ignore \"*.internal/**\"`.",
            |c| c.ignore.clone().into(),
            |c, n, t| {
                c.ignore = n.words(t)?;
                Ok(())
            },
        ),
        (
            "accept",
            Numbers,
            "Status codes that count as alive, beyond the 2xx and 3xx that always do.",
            |c| c.accept.clone().into(),
            |c, n, t| {
                c.accept = n.bounds::<u16>(t, 100, 599)?;
                Ok(())
            },
        ),
    ]);
}
