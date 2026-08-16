//! Announcing the built site to external destinations.
//!
//! A destination is one [`Backend`] impl over a [`SiteView`], plus one line in
//! [`Announce::configured`].

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
    pub config: &'a Config,
    pub documents: Vec<Doc>,
}

/// One publishable page, reduced to the metadata any destination might want.
pub struct Doc {
    /// Root-relative permalink, e.g. `/posts/hello/`.
    pub path: String,
    pub title: String,
    pub description: Option<String>,
    pub date: Option<time::Date>,
    /// Taxonomy terms across every taxonomy, flattened.
    pub tags: Vec<String>,
}

impl From<&Page> for Doc {
    fn from(page: &Page) -> Self {
        let fm = &page.frontmatter;
        Self {
            path: page.permalink.clone(),
            title: page.title().to_owned(),
            description: fm.blurb().map(str::to_owned),
            date: fm.date,
            tags: fm.taxonomies.values().flatten().cloned().collect(),
        }
    }
}

/// The `announce` command: which destinations a run targets, and what it hands
/// them.
pub struct Announce;

impl Announce {
    /// Announces to every configured destination in turn, erroring if none is
    /// configured.
    pub fn run(config: &Config, opts: &Options, ui: &Ui) -> Result<()> {
        let backends = Self::configured(config);
        if backends.is_empty() {
            return Err(AnnounceError::Unconfigured.into());
        }
        let site = Self::view(config)?;
        remote::publish(
            "announce",
            backends,
            &site,
            |site| Count::documents(site.documents.len()).to_string(),
            opts,
            ui,
        )
    }

    /// The enabled destinations, from config alone.
    fn configured(config: &Config) -> Vec<Box<dyn Backend<SiteView<'_>>>> {
        let mut out: Vec<Box<dyn Backend<SiteView<'_>>>> = Vec::new();
        if let Some(standard) = &config.announce.standard {
            out.push(Box::new(Standard::new(standard.clone())));
        }
        out
    }

    /// The eligible content pages as a [`SiteView`]; generated index and
    /// taxonomy pages are navigation, not publishable documents.
    fn view(config: &Config) -> Result<SiteView<'_>> {
        let theme = crate::theme::Theme::of(config)?;
        let project =
            crate::world::Project::new(config, crate::world::Mode::Build, theme.as_ref())?;
        let collections = discover(config, &project)?;
        let documents = collections
            .iter()
            .flat_map(|c| c.pages.iter())
            .filter(|page| page.eligible(config) && page.listed(config))
            .map(Doc::from)
            .collect();
        Ok(SiteView { config, documents })
    }
}

/// A disposable, per-backend map from record identifier to a fingerprint of
/// the content last sent, so an unchanged record is not re-sent; losing it
/// costs an idempotent re-send, never correctness.
#[derive(Default, Serialize, Deserialize)]
pub struct SkipCache {
    hashes: BTreeMap<String, Hash>,
}

impl SkipCache {
    /// Loads the cache for `backend`, treating any read or parse failure as
    /// empty.
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

    pub fn set(&mut self, id: String, fingerprint: Hash) {
        self.hashes.insert(id, fingerprint);
    }

    /// Drops every entry whose id is not in `keep`.
    pub fn retain(&mut self, keep: &BTreeSet<String>) {
        self.hashes.retain(|id, _| keep.contains(id));
    }

    pub fn save(&self, backend: &str) -> Result<()> {
        let bytes = serde_json::to_vec(self).map_err(|e| {
            crate::error::SerializeError::new(crate::error::Artifact::AnnounceCache, e)
        })?;
        crate::fs::write_all(Self::path(backend), bytes)
    }

    fn path(backend: &str) -> PathBuf {
        Config::scratch(crate::config::Scratch::Announce).join(format!("{backend}.json"))
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
