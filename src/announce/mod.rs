//! Announcing the built site to external destinations.
//!
//! The layer is backend-neutral: [`Backend`] is the one interface a
//! destination implements, and it receives a [`SiteView`] — the site reduced to
//! portable metadata, with no knowledge of any particular protocol. Concrete
//! backends (e.g. [`standard`], which speaks AT Protocol) map that view onto
//! their own records. Adding a destination is one `impl Backend` plus one line
//! in [`configured`]; nothing else in the codebase learns about it.

pub mod standard;

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::config::Config;
use crate::content::{Page, discover};
use crate::error::{AnnounceError, Result};
use crate::graph::Hash;
use crate::ui::{Count, Ui};

use self::standard::Standard;

/// How a announce run talks to the user — confirmations and interactive secret
/// entry. The announce layer depends only on this interface, never on the
/// terminal, so the CLI implements it with the shared prompt widgets and tests
/// pass a headless stub. The one seam through which announcing becomes
/// interactive.
pub trait Interaction {
    /// Confirm a mutating action; `Ok(false)` cancels it.
    fn confirm(&self, prompt: &str) -> Result<bool>;

    /// Prompt for a secret labeled `label`, or `Ok(None)` when the environment
    /// cannot supply one (non-interactive).
    fn secret(&self, label: &str) -> Result<Option<String>>;
}

/// Cross-cutting options for a announce run, backend-neutral. A backend reads
/// `dry_run` to preview without writing and resolves its own secret through
/// [`Options::secret`]; confirmation runs generically before a backend does.
pub struct Options<'a> {
    /// Report what would change without writing to any destination.
    pub dry_run: bool,
    /// Skip the confirmation prompt.
    pub yes: bool,
    /// A secret supplied on the command line — preferred over the environment
    /// variable and the interactive prompt.
    pub secret: Option<String>,
    /// The user-interaction backend (terminal in the CLI, a stub in tests).
    pub interaction: &'a dyn Interaction,
}

impl Options<'_> {
    /// Resolve a destination's secret: the CLI value (or stdin when it is the
    /// conventional `-`), else the `env` variable, else an interactive prompt
    /// labeled `label`. The one place credential acquisition lives, shared by
    /// every backend.
    pub fn secret(&self, env: &str, label: &str) -> Result<String> {
        if let Some(secret) = &self.secret {
            if secret != "-" {
                return Ok(secret.clone());
            }
            // A closed or blank stdin is "no secret", not an empty password —
            // matches the env and prompt branches, which both reject empty.
            let line = stdin_line()?;
            if line.is_empty() {
                return Err(AnnounceError::MissingSecret {
                    label: label.to_owned(),
                }
                .into());
            }
            return Ok(line);
        }
        if let Ok(secret) = std::env::var(env)
            && !secret.is_empty()
        {
            return Ok(secret);
        }
        self.interaction.secret(label)?.ok_or_else(|| {
            AnnounceError::MissingSecret {
                label: label.to_owned(),
            }
            .into()
        })
    }

    /// Confirm a mutating action, short-circuiting to `true` under `--yes`.
    fn confirm(&self, prompt: &str) -> Result<bool> {
        if self.yes {
            return Ok(true);
        }
        self.interaction.confirm(prompt)
    }
}

/// Read a single line from stdin as a secret — the conventional `-` value for a
/// secret flag (`--password -`), for piping without exposing it in argv. The
/// trailing newline is stripped; the rest is taken verbatim.
fn stdin_line() -> Result<String> {
    use std::io::BufRead;
    let mut line = String::new();
    std::io::stdin().lock().read_line(&mut line)?;
    Ok(line.trim_end_matches(['\r', '\n']).to_owned())
}

/// A backend-neutral view of the built site handed to every [`Backend`].
pub struct SiteView<'a> {
    /// The full resolved config — a backend reads the base `url`, `site` name,
    /// output directory, and its own `announce` block from here.
    pub config: &'a Config,
    /// Every publishable page, reduced to portable metadata.
    pub documents: Vec<Doc>,
}

/// One publishable page, reduced to the metadata any destination might want.
/// Typed throughout — dates stay [`time::Date`], not strings — so a backend
/// formats them however its wire format requires.
pub struct Doc {
    /// Root-relative permalink, e.g. `/posts/hello/`.
    pub path: String,
    /// Display title.
    pub title: String,
    /// Short description/summary, if the page carries one.
    pub description: Option<String>,
    /// Publication date, if dated.
    pub date: Option<time::Date>,
    /// Taxonomy terms across every taxonomy, flattened.
    pub tags: Vec<String>,
}

impl Doc {
    fn from_page(page: &Page) -> Self {
        let fm = &page.frontmatter;
        Self {
            path: page.permalink.clone(),
            title: page.title().to_owned(),
            description: fm.text("description").or_else(|| fm.text("summary")),
            date: fm.date,
            tags: fm.taxonomies.values().flatten().cloned().collect(),
        }
    }
}

/// A destination the built site can be announced to.
pub trait Backend {
    /// Stable, human-facing name, shown in progress output.
    fn name(&self) -> &'static str;

    /// Announce `site` under `opts`, reporting progress as it goes. Honors
    /// `opts.dry_run` by computing and reporting the plan without writing.
    fn run(&self, site: &SiteView, opts: &Options, ui: &Ui) -> Result<()>;
}

/// Announce to every configured destination in turn. Errors if none is
/// configured, so `baudelaire announce` on an unconfigured project explains
/// itself rather than silently doing nothing.
pub fn run(config: &Config, opts: &Options, ui: &Ui) -> Result<()> {
    let backends = configured(config);
    if backends.is_empty() {
        return Err(AnnounceError::Unconfigured.into());
    }
    let site = view(config)?;
    for backend in backends {
        ui.section(format_args!(
            "{} — {}",
            backend.name(),
            Count::documents(site.documents.len())
        ));
        // Confirm before any network mutation, unless previewing or `--yes`.
        if !opts.dry_run && !opts.confirm(&format!("announce to {}?", backend.name()))? {
            ui.detail(format_args!("skipped {}", backend.name()));
            continue;
        }
        backend.run(&site, opts, ui)?;
    }
    Ok(())
}

/// The enabled destinations, from config alone. THE single source of what a
/// `announce` run targets: add a backend by adding one line here.
fn configured(config: &Config) -> Vec<Box<dyn Backend>> {
    let mut out: Vec<Box<dyn Backend>> = Vec::new();
    if let Some(standard) = &config.announce.standard {
        out.push(Box::new(Standard::new(standard.clone())));
    }
    out
}

/// Reduce the discovered, eligible content pages to a [`SiteView`]. Only real
/// content pages are included — generated index and taxonomy pages are site
/// navigation, not publishable documents.
fn view(config: &Config) -> Result<SiteView<'_>> {
    let project = crate::world::Project::new(config, crate::world::Mode::Build)?;
    let collections = discover(config, &project)?;
    let documents = collections
        .iter()
        .flat_map(|c| c.pages.iter())
        .filter(|page| page.eligible(config))
        .map(Doc::from_page)
        .collect();
    Ok(SiteView { config, documents })
}

/// A disposable, per-backend skip-cache mapping a record identifier to a
/// fingerprint of the content last sent, so an unchanged record is not re-sent.
///
/// Kept under [`Config::SCRATCH`], so `clean` wipes it — deliberately. Its loss
/// only costs a re-send (which is idempotent), never correctness: a backend
/// diffs against the *remote* to decide deletions, so nothing is ever orphaned
/// because a local cache went missing.
#[derive(Default, Serialize, Deserialize)]
pub struct SkipCache {
    hashes: BTreeMap<String, Hash>,
}

impl SkipCache {
    /// Load the cache for `backend`, treating any read/parse failure as empty —
    /// a stale or missing cache is never fatal.
    pub fn load(backend: &str) -> Self {
        crate::fs::read(Self::path(backend))
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    }

    /// Whether `id` was last sent with this exact `fingerprint`.
    pub fn unchanged(&self, id: &str, fingerprint: &Hash) -> bool {
        self.hashes.get(id) == Some(fingerprint)
    }

    /// Record that `id` now holds content of `fingerprint`.
    pub fn set(&mut self, id: String, fingerprint: Hash) {
        self.hashes.insert(id, fingerprint);
    }

    /// Drop every entry whose id is not in `keep` — the records that no longer
    /// exist after this announce.
    pub fn retain(&mut self, keep: &BTreeSet<String>) {
        self.hashes.retain(|id, _| keep.contains(id));
    }

    /// Persist the cache for `backend`.
    pub fn save(&self, backend: &str) -> Result<()> {
        let bytes = serde_json::to_vec(self).map_err(|e| {
            crate::error::SerializeError::new(crate::error::Artifact::AnnounceCache, e)
        })?;
        crate::fs::write_all(Self::path(backend), bytes)
    }

    fn path(backend: &str) -> PathBuf {
        Config::scratch("announce").join(format!("{backend}.json"))
    }
}

#[cfg(test)]
mod tests {
    use super::{Hash, Interaction, Options, AnnounceError, Result, SkipCache};

    /// A headless [`Interaction`] for tests: a fixed confirmation answer and an
    /// optional prompt secret.
    struct Stub {
        confirm: bool,
        secret: Option<String>,
    }

    impl Interaction for Stub {
        fn confirm(&self, _prompt: &str) -> Result<bool> {
            Ok(self.confirm)
        }
        fn secret(&self, _label: &str) -> Result<Option<String>> {
            Ok(self.secret.clone())
        }
    }

    fn options<'a>(secret: Option<String>, stub: &'a Stub) -> Options<'a> {
        Options {
            dry_run: false,
            yes: false,
            secret,
            interaction: stub,
        }
    }

    /// An env var name no test sets, so secret resolution falls past the env step.
    const UNSET: &str = "BAUDELAIRE_TEST_UNSET_SECRET";

    #[test]
    fn secret_prefers_the_cli_value() {
        let stub = Stub {
            confirm: true,
            secret: Some("prompted".into()),
        };
        let opts = options(Some("flag".into()), &stub);
        assert_eq!(opts.secret(UNSET, "pw").unwrap(), "flag");
    }

    #[test]
    fn secret_falls_back_to_the_prompt() {
        let stub = Stub {
            confirm: true,
            secret: Some("prompted".into()),
        };
        let opts = options(None, &stub);
        assert_eq!(opts.secret(UNSET, "pw").unwrap(), "prompted");
    }

    #[test]
    fn secret_missing_when_no_source_can_supply_it() {
        let stub = Stub {
            confirm: true,
            secret: None,
        };
        let opts = options(None, &stub);
        assert!(matches!(
            opts.secret(UNSET, "pw"),
            Err(crate::error::BaudelaireErrorKind::Announce(
                AnnounceError::MissingSecret { .. }
            ))
        ));
    }

    #[test]
    fn confirm_short_circuits_under_yes() {
        let stub = Stub {
            confirm: false,
            secret: None,
        };
        let opts = Options {
            yes: true,
            ..options(None, &stub)
        };
        assert!(opts.confirm("go?").unwrap());
    }

    #[test]
    fn skip_cache_matches_only_the_recorded_fingerprint() {
        let (h, other) = (Hash::of_bytes(b"h"), Hash::of_bytes(b"other"));
        let mut cache = SkipCache::default();
        assert!(!cache.unchanged("k", &h));
        cache.set("k".into(), h);
        assert!(cache.unchanged("k", &h));
        assert!(!cache.unchanged("k", &other));
    }

    #[test]
    fn skip_cache_retain_drops_removed_records() {
        let (one, two) = (Hash::of_bytes(b"1"), Hash::of_bytes(b"2"));
        let mut cache = SkipCache::default();
        cache.set("keep".into(), one);
        cache.set("gone".into(), two);
        cache.retain(&["keep".to_owned()].into_iter().collect());
        assert!(cache.unchanged("keep", &one));
        assert!(!cache.unchanged("gone", &two));
    }
}
