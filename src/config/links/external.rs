//! `links { external { } }`: how outbound links are verified over the network.

use std::time::Duration;

use crate::config::dispatch::Kind::{Number, Numbers, Texts, Time};
use crate::config::dispatch::{Block, Section, Switch};
use crate::config::node::NodeExt;
use crate::config::value::ValueExt;

/// The outbound link check: whether it runs, and the manners it runs with.
///
/// Every field here exists because the check talks to hosts nobody in this
/// project controls. A verification cache that cannot be shortened re-reports a
/// link the author has just fixed; a deadline that cannot be lengthened fails a
/// slow-but-alive host; a URL that cannot be excluded (a staging box, a site
/// behind a login) is a permanent finding nothing can clear. Before these, the
/// only answer to any of them was to turn the whole check off.
#[derive(Debug, Clone, Hash)]
pub struct ExternalConfig {
    /// Verify outbound `http(s)` links over the network at all.
    ///
    /// Read by `check` alone: a build stays offline and deterministic, so a
    /// flaky host or an airplane can never change what it produces. `check
    /// --external` turns it on for one run.
    pub enabled: bool,
    /// How long a URL that answered stays answered.
    ///
    /// A link that was alive last week almost certainly still is, and re-asking
    /// every host on every CI run is both slow and rude. Only successes are
    /// remembered, so shortening this never keeps reporting a link that has
    /// since been fixed.
    pub fresh: Duration,
    /// How long one request may take before it counts as unreachable.
    pub timeout: Duration,
    /// How many requests are in flight at once. `None` leaves it to the build's
    /// own thread pool, which is the right answer until a host starts answering
    /// 429 to a site that links it fifty times.
    pub concurrency: Option<usize>,
    /// Globs, matched against each URL without its scheme, that are never
    /// requested.
    ///
    /// The same glob grammar as `prune { keep }`, so `/` is a segment boundary
    /// and `**` is what crosses one: `*.internal/**`.
    pub ignore: Vec<String>,
    /// Status codes that count as the link working, beyond the 2xx and 3xx that
    /// always do: a page behind a login answers 401 and is still there.
    pub accept: Vec<u16>,
}

impl ExternalConfig {
    /// Whether `status` says the link is there, for a host that answered.
    ///
    /// One rule, on the config that carries the exception list, so the probe and
    /// any future reader cannot disagree about what a live link looks like.
    pub fn alive(&self, status: u16) -> bool {
        (200..400).contains(&status) || self.accept.contains(&status)
    }
}

impl Default for ExternalConfig {
    fn default() -> Self {
        Self {
            // Off: the check needs the network, so it can never be a default.
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
    const SWITCH: Option<Switch<Self>> = Some(|c, on| c.enabled = on);

    const RULES: Block<Self> = Block(&[
        (
            "fresh",
            Time,
            "How long a link that answered is trusted before it is asked again.",
            |c, n, t| {
                c.fresh = n.duration(t, 0)?;
                Ok(())
            },
        ),
        (
            "timeout",
            Time,
            "How long one request may take before the link counts as unreachable.",
            |c, n, t| {
                c.timeout = n.duration(t, 0)?;
                Ok(())
            },
        ),
        (
            "concurrency",
            Number,
            "How many links are fetched at once. Unset, as many as the build has threads.",
            |c, n, t| {
                // A pool of nothing would hang rather than throttle, so the
                // floor is one request at a time.
                let at_once: u16 = n.arg(t, 0)?.bounded(t, NodeExt::span(n), 1, u16::MAX)?;
                c.concurrency = Some(usize::from(at_once));
                Ok(())
            },
        ),
        (
            "ignore",
            Texts,
            "Globs, matched against each URL without its scheme, that are never requested: `ignore \"*.internal/**\"`.",
            |c, n, t| {
                c.ignore = n.words(t)?;
                Ok(())
            },
        ),
        (
            "accept",
            Numbers,
            "Status codes that count as alive, beyond the 2xx and 3xx that always do.",
            |c, n, t| {
                // The range a status code lives in: a `1000` here is a typo that
                // would otherwise sit in the list matching nothing.
                c.accept = n.bounds::<u16>(t, 100, 599)?;
                Ok(())
            },
        ),
    ]);
}
