//! Verifies outbound links over the network, for `check --external`.
//!
//! Never part of a build: a build must produce the same bytes offline, on a
//! plane, and when someone else's host is having a bad afternoon. This runs
//! only from [`crate::engine::Engine::check`], where reaching the network is
//! what the user asked for.
//!
//! Two outcomes, kept apart because only one is the site's fault: a URL that
//! answers 4xx/5xx is a dead link and fails the check, while a URL that cannot
//! be reached at all (DNS, TLS, timeout) is reported as a warning, since the
//! most likely cause is the network in between.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Duration;

use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use wax::{Glob, Program};

use super::Compiled;
use crate::config::{Config, ExternalConfig};
use crate::error::warning::{Unreachable, UnreachableLinks};
use crate::error::{ContentError, Dead, DeadLinks, Result};
use crate::ui::Ui;

/// The external link check.
pub(in crate::engine) struct External;

impl External {
    /// Verify every outbound link the compiled pages carry.
    pub(in crate::engine) fn run(site: &Compiled, ui: &Ui) -> Result<()> {
        let policy = &site.config.links.external;
        let ignored = Ignored::of(policy)?;
        // URL -> the pages that link to it, so a dead link names every place it
        // has to be fixed and is requested exactly once however often it appears.
        let mut targets: BTreeMap<&str, Vec<String>> = BTreeMap::new();
        for page in site.pages {
            for url in page.external.iter().filter(|url| !ignored.claims(url)) {
                targets.entry(url).or_default().push(page.label.clone());
            }
        }
        if targets.is_empty() {
            return Ok(());
        }

        let mut verified = Verified::load(site.config);
        let stale: Vec<&str> = targets
            .keys()
            .copied()
            .filter(|url| !verified.is_fresh(url, policy.fresh))
            .collect();
        ui.detail(format_args!(
            "checking {} of {} outbound link{} ({} still verified)",
            stale.len(),
            targets.len(),
            if targets.len() == 1 { "" } else { "s" },
            targets.len() - stale.len()
        ));

        let progress = ui.progress("checking", stale.len());
        let agent = Self::agent(policy);
        let probe = || {
            stale
                .par_iter()
                .map(|url| {
                    let probe = Probe::of(&agent, url, policy);
                    progress.tick((*url).to_owned());
                    (*url, probe)
                })
                .collect()
        };
        // A site that names a concurrency asks for *these* requests to be
        // throttled, not for the rest of the build to be: a pool of its own is
        // what keeps the limit off every other parallel pass.
        let probed: Vec<(&str, Probe)> = match policy.concurrency {
            Some(threads) => match rayon::ThreadPoolBuilder::new().num_threads(threads).build() {
                Ok(pool) => pool.install(probe),
                // Spawning threads is the only way this fails, and the global
                // pool the fallback uses is made of the same threads: there is
                // no second thing to try, and failing the check over it would
                // report the machine as a dead link. Recorded rather than
                // silent, as `Verified::save` is.
                Err(e) => {
                    tracing::debug!("link checker pool of {threads} not built: {e}");
                    probe()
                }
            },
            None => probe(),
        };
        progress.finish();

        let mut dead: Vec<Dead> = Vec::new();
        let mut unreachable: Vec<Unreachable> = Vec::new();
        for (url, probe) in probed {
            match probe {
                Probe::Alive => verified.record(url),
                Probe::Status(status) => dead.push(Dead {
                    url: url.to_owned(),
                    status,
                    // Every url in `probed` came from `targets`, so this is
                    // never the empty fallback.
                    pages: targets.get(url).cloned().unwrap_or_default(),
                }),
                Probe::Unreachable(why) => unreachable.push(Unreachable {
                    url: url.to_owned(),
                    why,
                }),
            }
        }
        verified.save(site.config);

        if !unreachable.is_empty() {
            ui.warn(UnreachableLinks::from(unreachable));
        }
        match dead.is_empty() {
            true => Ok(()),
            false => Err(DeadLinks::from(dead).into()),
        }
    }

    /// The agent every probe shares: one connection pool, the site's own
    /// deadline, and a user agent that tells an administrator who is knocking.
    ///
    /// [`Status::Read`] because a 404 *is* the answer here, not a transport
    /// failure to be unwrapped out of an error type.
    fn agent(policy: &ExternalConfig) -> ureq::Agent {
        crate::remote::Http::within("link checker", crate::remote::Status::Read, policy.timeout)
    }
}

/// The URLs `links { external { ignore } }` says never to request.
///
/// Compiled once per run rather than per URL, in the glob grammar `prune
/// { keep }` uses, so there is one glob dialect in the project rather than a
/// second one for URLs.
struct Ignored<'a>(Vec<Glob<'a>>);

impl<'a> Ignored<'a> {
    /// The compiled patterns, or a precise error naming the one that is not a
    /// glob. Checked here rather than at config parse for the same reason
    /// `prune { keep }` is: the grammar belongs to `wax`, and its error carries
    /// the offset within the pattern.
    fn of(policy: &'a ExternalConfig) -> Result<Self> {
        policy
            .ignore
            .iter()
            .map(|pattern| {
                Glob::new(pattern)
                    .map_err(|e| ContentError::bad_glob("external", pattern, e).into())
            })
            .collect::<Result<Vec<_>>>()
            .map(Self)
    }

    /// Whether any pattern claims `url`.
    fn claims(&self, url: &str) -> bool {
        let rest = Self::unschemed(url);
        self.0.iter().any(|glob| glob.is_match(rest))
    }

    /// A URL without its `scheme://`, which is what a pattern is matched
    /// against.
    ///
    /// Two reasons, and the second is why this is not a compromise. A glob
    /// cannot carry the scheme anyway -- `//` is two adjacent component
    /// boundaries, which the grammar refuses -- and a site excluding a host
    /// means the host, not one way of addressing it: `*.internal/**` covers
    /// both spellings without writing the pattern twice.
    fn unschemed(url: &str) -> &str {
        url.split_once("://").map_or(url, |(_, rest)| rest)
    }
}

/// What one request found.
enum Probe {
    Alive,
    /// The host answered, and said no.
    Status(u16),
    /// Nothing answered: DNS, TLS, a timeout, a refused connection.
    Unreachable(String),
}

impl Probe {
    /// Probe a URL with `HEAD`, falling back to `GET`.
    ///
    /// Plenty of servers answer `HEAD` with 403, 405, or 501 while serving the
    /// page perfectly well, so a rejection of the *method* is not an answer
    /// about the *link* and has to be asked again properly.
    /// A status the site accepts is checked *before* the method fallback, so
    /// `accept 403` costs one request rather than two: with 403 in both sets,
    /// asking again with `GET` can only arrive at the same answer.
    fn of(agent: &ureq::Agent, url: &str, policy: &ExternalConfig) -> Self {
        match Self::request(agent.head(url).call(), policy) {
            Self::Status(code) if Self::method_rejected(code) => {
                Self::request(agent.get(url).call(), policy)
            }
            probe => probe,
        }
    }

    /// Whether a status says "not like that" rather than "not there":
    /// 403 (hosts and CDNs that gate anything but `GET`), 405 (the method is
    /// not allowed) and 501 (the server never implemented it).
    fn method_rejected(code: u16) -> bool {
        const REJECTED: [u16; 3] = [403, 405, 501];
        REJECTED.contains(&code)
    }

    /// A 2xx or 3xx answer is the link working, as is anything the site added to
    /// `accept`: a redirect that resolves is a live target, and following the
    /// chain is the agent's business, not this pass's.
    fn request(
        result: Result<ureq::http::Response<ureq::Body>, ureq::Error>,
        policy: &ExternalConfig,
    ) -> Self {
        match result {
            Ok(response) => {
                let status = response.status().as_u16();
                match policy.alive(status) {
                    true => Self::Alive,
                    false => Self::Status(status),
                }
            }
            Err(e) => Self::Unreachable(e.to_string()),
        }
    }
}

/// URLs that answered, and when. Only successes are remembered: caching a
/// failure would keep reporting a link that has since been fixed.
#[derive(Default, Serialize, Deserialize)]
struct Verified(BTreeMap<String, i64>);

impl Verified {
    /// Where the record lives: under the scratch directory, so `clean` wipes it
    /// and nothing here is ever mistaken for build output.
    fn path(config: &Config) -> PathBuf {
        config.root.join(Config::scratch("links")).join("seen.json")
    }

    /// Load the previous run's record. Unreadable or corrupt is not an error:
    /// the worst case is re-checking every link, which is what a first run does
    /// anyway.
    fn load(config: &Config) -> Self {
        std::fs::read_to_string(Self::path(config))
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    /// Whether `url` was verified within `fresh` and can be skipped.
    ///
    /// A timestamp from the future counts as unknown: a clock that jumped
    /// backwards must not freeze every link as permanently verified.
    fn is_fresh(&self, url: &str, fresh: Duration) -> bool {
        let Some(&at) = self.0.get(url) else {
            return false;
        };
        let age = Self::now() - at;
        (0..fresh.as_secs().cast_signed()).contains(&age)
    }

    /// Now, in the unix seconds the record is keyed by.
    fn now() -> i64 {
        OffsetDateTime::now_utc().unix_timestamp()
    }

    fn record(&mut self, url: &str) {
        self.0.insert(url.to_owned(), Self::now());
    }

    /// Persist the record, best-effort: failing to write a cache must not fail
    /// a check that otherwise passed.
    fn save(&self, config: &Config) {
        let path = Self::path(config);
        let written = path
            .parent()
            .map(std::fs::create_dir_all)
            .transpose()
            .and_then(|_| serde_json::to_vec(self).map_err(std::io::Error::other))
            .and_then(|json| std::fs::write(&path, json));
        if let Err(e) = written {
            tracing::debug!(path = %path.display(), "link record not saved: {e}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The default window, which the cases below are written against.
    fn fresh() -> Duration {
        ExternalConfig::default().fresh
    }

    #[test]
    fn a_recent_verification_is_fresh_and_an_old_one_is_not() {
        let now = OffsetDateTime::now_utc().unix_timestamp();
        let mut verified = Verified::default();
        verified.0.insert("https://fresh.test".into(), now - 60);
        verified.0.insert(
            "https://stale.test".into(),
            now - fresh().as_secs().cast_signed() - 1,
        );

        assert!(verified.is_fresh("https://fresh.test", fresh()));
        assert!(!verified.is_fresh("https://stale.test", fresh()));
        assert!(!verified.is_fresh("https://unknown.test", fresh()));
    }

    /// The window is the site's, so shortening it re-asks a link the default
    /// would still be trusting.
    #[test]
    fn a_shorter_window_makes_a_verification_stale() {
        let mut verified = Verified::default();
        let now = OffsetDateTime::now_utc().unix_timestamp();
        verified.0.insert("https://a.test".into(), now - 3600);
        assert!(verified.is_fresh("https://a.test", fresh()));
        assert!(!verified.is_fresh("https://a.test", Duration::from_mins(5)));
    }

    /// A clock that jumped backwards must not make everything permanently
    /// fresh, so a future timestamp is treated as unknown.
    #[test]
    fn a_timestamp_from_the_future_is_not_fresh() {
        let mut verified = Verified::default();
        let ahead = OffsetDateTime::now_utc().unix_timestamp() + 3600;
        verified.0.insert("https://ahead.test".into(), ahead);
        assert!(!verified.is_fresh("https://ahead.test", fresh()));
    }

    #[test]
    fn only_a_method_rejection_earns_a_second_request() {
        for code in [403, 405, 501] {
            assert!(Probe::method_rejected(code), "{code}");
        }
        for code in [404, 410, 500, 503] {
            assert!(!Probe::method_rejected(code), "{code}");
        }
    }

    /// 2xx and 3xx always live; everything else needs the site to say so.
    #[test]
    fn accept_widens_what_counts_as_alive() {
        let mut policy = ExternalConfig::default();
        assert!(policy.alive(200));
        assert!(policy.alive(301));
        assert!(!policy.alive(401));
        assert!(!policy.alive(404));

        policy.accept = vec![401, 429];
        assert!(policy.alive(401));
        assert!(policy.alive(429));
        assert!(!policy.alive(404));
    }

    /// The pattern is matched against the URL without its scheme, so it names a
    /// host and a path the way `prune { keep }` names a path: `/` is a segment
    /// boundary, and `**` is what crosses one.
    #[test]
    fn ignore_claims_the_urls_it_names() {
        let policy = ExternalConfig {
            ignore: vec!["*.internal/**".into(), "one.test/**".into()],
            ..ExternalConfig::default()
        };
        let ignored = Ignored::of(&policy).expect("the patterns are globs");

        assert!(ignored.claims("https://box.internal/health"));
        assert!(ignored.claims("https://one.test/a/b"));
        assert!(!ignored.claims("https://other.test/a"));
        // A host is a host however it is addressed: one pattern, both schemes.
        assert!(ignored.claims("http://box.internal/health"));
    }

    /// A pattern that is not a glob is the author's mistake, named as one rather
    /// than quietly matching nothing.
    #[test]
    fn a_pattern_that_is_not_a_glob_is_an_error() {
        let policy = ExternalConfig {
            ignore: vec!["{unclosed".into()],
            ..ExternalConfig::default()
        };
        assert!(Ignored::of(&policy).is_err());
    }
}
