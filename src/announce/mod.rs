//! Announcing the built site to external destinations.
//!
//! The layer is backend-neutral: [`Backend`] is the one interface a
//! destination implements, and it receives a [`SiteView`]: the site reduced to
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
use crate::remote::{self, Backend, Options};
use crate::ui::{Count, Ui};

use self::standard::Standard;

/// A backend-neutral view of the built site handed to every [`Backend`].
pub struct SiteView<'a> {
    /// The full resolved config: a backend reads the base `url`, `site` name,
    /// output directory, and its own `announce` block from here.
    pub config: &'a Config,
    /// Every publishable page, reduced to portable metadata.
    pub documents: Vec<Doc>,
}

/// One publishable page, reduced to the metadata any destination might want.
/// Typed throughout (dates stay [`time::Date`], not strings), so a backend
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

/// Announce to every configured destination in turn. Errors if none is
/// configured, so `baudelaire announce` on an unconfigured project explains
/// itself rather than silently doing nothing.
pub fn run(config: &Config, opts: &Options, ui: &Ui) -> Result<()> {
    let backends = configured(config);
    if backends.is_empty() {
        return Err(AnnounceError::Unconfigured.into());
    }
    let site = view(config)?;
    remote::publish(
        "announce",
        backends,
        &site,
        |site| Count::documents(site.documents.len()).to_string(),
        opts,
        ui,
    )
}

/// The enabled destinations, from config alone. THE single source of what a
/// `announce` run targets: add a backend by adding one line here.
fn configured(config: &Config) -> Vec<Box<dyn Backend<SiteView<'_>>>> {
    let mut out: Vec<Box<dyn Backend<SiteView<'_>>>> = Vec::new();
    if let Some(standard) = &config.announce.standard {
        out.push(Box::new(Standard::new(standard.clone())));
    }
    out
}

/// Reduce the discovered, eligible content pages to a [`SiteView`]. Only real
/// content pages are included: generated index and taxonomy pages are site
/// navigation, not publishable documents.
fn view(config: &Config) -> Result<SiteView<'_>> {
    let project = crate::world::Project::new(config, crate::world::Mode::Build)?;
    let collections = discover(config, &project)?;
    let documents = collections
        .iter()
        .flat_map(|c| c.pages.iter())
        .filter(|page| page.eligible(config) && page.listed(config))
        .map(Doc::from_page)
        .collect();
    Ok(SiteView { config, documents })
}

/// A disposable, per-backend skip-cache mapping a record identifier to a
/// fingerprint of the content last sent, so an unchanged record is not re-sent.
///
/// Kept under [`Config::SCRATCH`], so `clean` wipes it, deliberately. Its loss
/// only costs a re-send (which is idempotent), never correctness: a backend
/// diffs against the *remote* to decide deletions, so nothing is ever orphaned
/// because a local cache went missing.
#[derive(Default, Serialize, Deserialize)]
pub struct SkipCache {
    hashes: BTreeMap<String, Hash>,
}

impl SkipCache {
    /// Load the cache for `backend`, treating any read/parse failure as empty:
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

    /// Drop every entry whose id is not in `keep`: the records that no longer
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
    use super::{Hash, SkipCache};

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
        cache.retain(&std::iter::once("keep".to_owned()).collect());
        assert!(cache.unchanged("keep", &one));
        assert!(!cache.unchanged("gone", &two));
    }
}
