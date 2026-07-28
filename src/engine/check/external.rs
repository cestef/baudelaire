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

use super::Compiled;
use crate::config::Config;
use crate::error::warning::{Unreachable, UnreachableLinks};
use crate::error::{Dead, DeadLinks, Result};
use crate::ui::Ui;

/// How long a verified URL stays verified. A link that answered last week is
/// almost certainly still there, and re-asking every host on every CI run is
/// both slow and rude.
const FRESH: Duration = Duration::from_secs(7 * 24 * 60 * 60);

/// Per-request ceiling. Generous enough for a slow host, short enough that one
/// black hole does not hold up the run.
const TIMEOUT: Duration = Duration::from_secs(10);

/// The external link check.
pub(in crate::engine) struct External;

impl External {
    /// Verify every outbound link the compiled pages carry.
    pub(in crate::engine) fn run(site: &Compiled, ui: &Ui) -> Result<()> {
        // URL -> the pages that link to it, so a dead link names every place it
        // has to be fixed and is requested exactly once however often it appears.
        let mut targets: BTreeMap<&str, Vec<String>> = BTreeMap::new();
        for page in site.pages {
            for url in page.external {
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
            .filter(|url| !verified.is_fresh(url))
            .collect();
        ui.detail(format_args!(
            "checking {} of {} outbound link{} ({} still verified)",
            stale.len(),
            targets.len(),
            if targets.len() == 1 { "" } else { "s" },
            targets.len() - stale.len()
        ));

        let progress = ui.progress("checking", stale.len());
        let agent = Self::agent();
        let probed: Vec<(&str, Probe)> = stale
            .par_iter()
            .map(|url| {
                let probe = Probe::of(&agent, url);
                progress.tick((*url).to_string());
                (*url, probe)
            })
            .collect();
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

    /// The agent every probe shares: one connection pool, a bounded wait, and a
    /// user agent that tells an administrator who is knocking.
    ///
    /// `http_status_as_error(false)` because a 404 *is* the answer here, not a
    /// transport failure to be unwrapped out of an error type.
    fn agent() -> ureq::Agent {
        ureq::Agent::config_builder()
            .http_status_as_error(false)
            .timeout_global(Some(TIMEOUT))
            .user_agent(format!("baudelaire/{} (link checker)", crate::VERSION))
            .tls_config(crate::remote::tls())
            .build()
            .into()
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
    fn of(agent: &ureq::Agent, url: &str) -> Self {
        match Self::request(agent.head(url).call()) {
            Self::Status(code) if Self::method_rejected(code) => {
                Self::request(agent.get(url).call())
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

    /// A 2xx or 3xx answer is the link working: a redirect that resolves is a
    /// live target, and following the chain is the agent's business, not this
    /// pass's.
    fn request(result: Result<ureq::http::Response<ureq::Body>, ureq::Error>) -> Self {
        match result {
            Ok(response) => {
                let status = response.status();
                match status.is_success() || status.is_redirection() {
                    true => Self::Alive,
                    false => Self::Status(status.as_u16()),
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

    /// Whether `url` was verified recently enough to skip.
    ///
    /// A timestamp from the future counts as unknown: a clock that jumped
    /// backwards must not freeze every link as permanently verified.
    fn is_fresh(&self, url: &str) -> bool {
        let Some(&at) = self.0.get(url) else {
            return false;
        };
        let age = Self::now() - at;
        (0..FRESH.as_secs() as i64).contains(&age)
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

    #[test]
    fn a_recent_verification_is_fresh_and_an_old_one_is_not() {
        let now = OffsetDateTime::now_utc().unix_timestamp();
        let mut verified = Verified::default();
        verified.0.insert("https://fresh.test".into(), now - 60);
        verified.0.insert(
            "https://stale.test".into(),
            now - FRESH.as_secs() as i64 - 1,
        );

        assert!(verified.is_fresh("https://fresh.test"));
        assert!(!verified.is_fresh("https://stale.test"));
        assert!(!verified.is_fresh("https://unknown.test"));
    }

    /// A clock that jumped backwards must not make everything permanently
    /// fresh, so a future timestamp is treated as unknown.
    #[test]
    fn a_timestamp_from_the_future_is_not_fresh() {
        let mut verified = Verified::default();
        let ahead = OffsetDateTime::now_utc().unix_timestamp() + 3600;
        verified.0.insert("https://ahead.test".into(), ahead);
        assert!(!verified.is_fresh("https://ahead.test"));
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
}
