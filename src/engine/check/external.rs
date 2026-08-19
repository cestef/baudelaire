//! Verifies outbound links over the network, for `check --external` alone: a
//! build must produce the same bytes offline.

use std::collections::BTreeMap;
use std::path::PathBuf;

use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use wax::{Glob, Program};

use super::Compiled;
use crate::config::{Config, ExternalConfig};
use crate::error::warning::{Unreachable, UnreachableLinks};
use crate::error::{ContentError, Dead, DeadLinks, Result};
use crate::ui::Ui;

pub(in crate::engine) struct External;

impl External {
    /// Verify every outbound link the compiled pages carry.
    pub(in crate::engine) fn run(site: &Compiled, ui: &Ui) -> Result<()> {
        let policy = &site.config.check.external;
        let ignored = Ignored::of(policy)?;
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
            .filter(|url| !verified.is_fresh(url, policy))
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
        let probed: Vec<(&str, Probe)> = policy.concurrency.map_or_else(probe, |threads| {
            match rayon::ThreadPoolBuilder::new().num_threads(threads).build() {
                Ok(pool) => pool.install(probe),
                Err(e) => {
                    tracing::debug!("link checker pool of {threads} not built: {e}");
                    probe()
                }
            }
        });
        progress.finish();

        let mut dead: Vec<Dead> = Vec::new();
        let mut unreachable: Vec<Unreachable> = Vec::new();
        for (url, probe) in probed {
            match probe {
                Probe::Alive(status) => verified.record(url, status),
                Probe::Status(status) => dead.push(Dead {
                    url: url.to_owned(),
                    status,
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
        if dead.is_empty() {
            Ok(())
        } else {
            Err(DeadLinks::from(dead).into())
        }
    }

    /// The agent every probe shares: one connection pool, the site's own
    /// deadline, and a status posture that reads a 404 as an answer rather than
    /// a transport error.
    fn agent(policy: &ExternalConfig) -> ureq::Agent {
        crate::remote::Http::within("link checker", crate::remote::Status::Read, policy.timeout)
    }
}

/// The URLs `links { external { ignore } }` says never to request, compiled
/// once per run in the glob grammar `prune { keep }` uses.
struct Ignored<'a>(Vec<Glob<'a>>);

impl<'a> Ignored<'a> {
    /// The compiled patterns, or a precise error naming the one that is not a
    /// glob.
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
    /// against: a site excluding a host means the host, not one way of
    /// addressing it.
    fn unschemed(url: &str) -> &str {
        url.split_once("://").map_or(url, |(_, rest)| rest)
    }
}

/// What one request found.
enum Probe {
    /// The host answered something the site calls alive, and what that was.
    Alive(u16),
    /// The host answered, and said no.
    Status(u16),
    /// Nothing answered: DNS, TLS, a timeout, a refused connection.
    Unreachable(String),
}

impl Probe {
    /// Probe a URL with `HEAD`, falling back to `GET` when the answer rejects
    /// the *method* rather than the link. A status the site already accepts is
    /// taken before that fallback, so `accept 403` costs one request.
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
                if policy.alive(status) {
                    Self::Alive(status)
                } else {
                    Self::Status(status)
                }
            }
            Err(e) => Self::Unreachable(e.to_string()),
        }
    }
}

/// One remembered verification: when the host answered, and what it answered.
/// The status is kept because "alive" is the *site's* judgement, so narrowing
/// `accept` has to re-ask a URL it once waved through.
#[derive(Clone, Copy, Serialize, Deserialize)]
struct Seen {
    at: i64,
    status: u16,
}

/// URLs that answered, and when. Only successes are remembered: caching a
/// failure would keep reporting a link that has since been fixed.
#[derive(Default, Serialize, Deserialize)]
struct Verified(BTreeMap<String, Seen>);

impl Verified {
    /// Where the record lives: under the scratch directory, so `clean` wipes it
    /// and nothing here is ever mistaken for build output.
    fn path(config: &Config) -> PathBuf {
        config
            .root
            .join(Config::scratch(crate::config::Scratch::Links))
            .join("seen.json")
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

    /// Whether `url` was verified recently enough to skip, *and* answered
    /// something this run's policy still calls alive. A timestamp from the
    /// future counts as unknown, so a clock that jumped backwards cannot freeze
    /// a link as permanently verified.
    fn is_fresh(&self, url: &str, policy: &ExternalConfig) -> bool {
        let Some(&seen) = self.0.get(url) else {
            return false;
        };
        let age = Self::now() - seen.at;
        policy.alive(seen.status) && (0..policy.fresh.as_secs().cast_signed()).contains(&age)
    }

    /// Now, in the unix seconds the record is keyed by.
    fn now() -> i64 {
        OffsetDateTime::now_utc().unix_timestamp()
    }

    fn record(&mut self, url: &str, status: u16) {
        self.0.insert(
            url.to_owned(),
            Seen {
                at: Self::now(),
                status,
            },
        );
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
    use std::time::Duration;

    use super::*;

    fn policy() -> ExternalConfig {
        ExternalConfig::default()
    }

    /// A record holding one URL last seen `secs` ago answering `status`.
    fn seen(url: &str, secs: i64, status: u16) -> Verified {
        let mut verified = Verified::default();
        verified.saw(url, secs, status);
        verified
    }

    impl Verified {
        fn saw(&mut self, url: &str, secs: i64, status: u16) {
            let at = OffsetDateTime::now_utc().unix_timestamp() - secs;
            self.0.insert(url.to_owned(), Seen { at, status });
        }
    }

    #[test]
    fn a_recent_verification_is_fresh_and_an_old_one_is_not() {
        let window = policy().fresh.as_secs().cast_signed();
        let mut verified = seen("https://fresh.test", 60, 200);
        verified.saw("https://stale.test", window + 1, 200);

        assert!(verified.is_fresh("https://fresh.test", &policy()));
        assert!(!verified.is_fresh("https://stale.test", &policy()));
        assert!(!verified.is_fresh("https://unknown.test", &policy()));
    }

    #[test]
    fn a_shorter_window_makes_a_verification_stale() {
        let verified = seen("https://a.test", 3600, 200);
        assert!(verified.is_fresh("https://a.test", &policy()));

        let brief = ExternalConfig {
            fresh: Duration::from_mins(5),
            ..policy()
        };
        assert!(!verified.is_fresh("https://a.test", &brief));
    }

    #[test]
    fn narrowing_accept_makes_a_verification_stale() {
        let mut verified = seen("https://gated.test", 60, 401);
        verified.saw("https://plain.test", 60, 200);

        let lenient = ExternalConfig {
            accept: vec![401],
            ..policy()
        };
        assert!(verified.is_fresh("https://gated.test", &lenient));
        assert!(!verified.is_fresh("https://gated.test", &policy()));
        assert!(verified.is_fresh("https://plain.test", &policy()));
    }

    #[test]
    fn a_timestamp_from_the_future_is_not_fresh() {
        let verified = seen("https://ahead.test", -3600, 200);
        assert!(!verified.is_fresh("https://ahead.test", &policy()));
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
        assert!(ignored.claims("http://box.internal/health"));
    }

    #[test]
    fn a_pattern_that_is_not_a_glob_is_an_error() {
        let policy = ExternalConfig {
            ignore: vec!["{unclosed".into()],
            ..ExternalConfig::default()
        };
        assert!(Ignored::of(&policy).is_err());
    }
}
