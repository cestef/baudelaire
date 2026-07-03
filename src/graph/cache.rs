//! Authoritative build cache: content hashes, dependency edges, and rendered
//! output, persisted between builds to drive incremental rebuilds.
//!
//! Layout under the cache directory:
//!
//! ```text
//! manifest.json          # small: config fingerprint + per-page metadata
//! objects/ab/abcd…       # rendered HTML, content-addressed by blob hash
//! ```
//!
//! The manifest holds only metadata — hashes, dependency edges, output paths,
//! and a pointer to each page's HTML *blob*. The HTML itself lives in a
//! content-addressed object store, so a load parses a small manifest instead of
//! deserializing every page's markup, identical output is stored once, and an
//! unchanged blob is never rewritten.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::config::Config;
use crate::content::Page;
use crate::error::{Artifact, Result, SerializeError};
use crate::graph::{Deps, Hash};
use crate::world::BuildContext;

/// The on-disk manifest file name under the cache directory.
const MANIFEST: &str = "manifest.json";

/// Subdirectory holding content-addressed HTML blobs.
const OBJECTS: &str = "objects";

/// A page's cached compile result and the fingerprints that validate it. The
/// rendered HTML is not inlined — [`Entry::blob`] points at it in the object
/// store, read only on a cache hit.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Entry {
    /// Hash of the page's own source.
    hash: Hash,
    /// Dependency files and their hashes at compile time.
    deps: BTreeMap<String, Hash>,
    /// Content hash of the rendered HTML; locates its blob in the object store.
    blob: Hash,
}

/// The serialized cache manifest — metadata only, no page markup.
#[derive(Debug, Default, Serialize, Deserialize)]
struct Manifest {
    /// Fingerprint of the inputs that produced these entries (config, build
    /// context, asset map). Any change invalidates the whole manifest — it can
    /// alter every permalink or embedded input.
    config: Option<Hash>,
    /// Entries keyed by page source path.
    pages: BTreeMap<String, Entry>,
}

/// The build cache. Loads the previous manifest, answers reuse queries, and
/// accumulates the next manifest as pages are reused or recompiled.
pub struct Cache {
    dir: PathBuf,
    enabled: bool,
    config: Hash,
    prev: Manifest,
    next: Manifest,
}

impl Cache {
    /// Load the cache for a build. When incremental builds are disabled the
    /// cache still records the next manifest but never reports a hit.
    ///
    /// The manifest fingerprint mixes the config, the build context, *and* the
    /// asset map, so a new commit, a new day, or a re-fingerprinted asset
    /// invalidates pages that embed `sys.inputs.baudelaire` or reference assets.
    /// Only the small manifest is read here — HTML blobs are fetched lazily on a
    /// hit.
    pub fn load(config: &Config, context: &BuildContext, assets: &Hash) -> Self {
        let dir = config.cache.dir.clone();
        let prev = fs::read(dir.join(MANIFEST))
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        let fingerprint = Hash::of(&(config, context, assets));
        Self {
            dir,
            enabled: config.cache.incremental,
            next: Manifest {
                config: Some(fingerprint.clone()),
                pages: BTreeMap::new(),
            },
            config: fingerprint,
            prev,
        }
    }

    /// Cached HTML for `page` if still valid — its content fingerprint, every
    /// dependency, and the manifest fingerprint are all unchanged, and its blob
    /// is still present in the object store. A hit carries the entry into the
    /// next manifest so it survives to the following build.
    ///
    /// `fingerprint` hashes the exact text typst compiles, so it validates
    /// generated pages (taxonomies, paginated indexes) too — their synthetic
    /// sources never touch disk and so have no file to hash.
    pub fn reuse(&mut self, page: &Page, fingerprint: &Hash) -> Option<String> {
        if !self.enabled || self.prev.config.as_ref() != Some(&self.config) {
            return None;
        }
        let entry = self.prev.pages.get(&Self::key(page))?;
        if &entry.hash != fingerprint {
            return None;
        }
        if !entry.deps.iter().all(|(path, hash)| {
            Hash::of_file(Path::new(path)).as_ref() == Some(hash)
        }) {
            return None;
        }
        let html = fs::read_to_string(self.object(&entry.blob)).ok()?;
        self.next.pages.insert(Self::key(page), entry.clone());
        Some(html)
    }

    /// Record a freshly compiled page against its content fingerprint and
    /// dependency hashes, staging its HTML for the object store.
    pub fn record(&mut self, page: &Page, fingerprint: Hash, html: &str, deps: &Deps) {
        let deps = deps
            .files()
            .iter()
            .filter_map(|p| Some((p.display().to_string(), Hash::of_file(p)?)))
            .collect();
        let blob = Hash::of_bytes(html.as_bytes());
        self.next.pages.insert(
            Self::key(page),
            Entry {
                hash: fingerprint,
                deps,
                blob,
            },
        );
    }

    /// Persist the manifest and every referenced HTML blob, then drop objects no
    /// longer referenced. Blobs are content-addressed and written write-once, so
    /// an unchanged page's markup is never rewritten. `outputs` supplies the HTML
    /// for freshly recorded pages (cache hits already have their blob on disk).
    pub fn save(&self, outputs: &[(&Page, &str)]) -> Result<()> {
        crate::fs::create_dir_all(&self.dir)?;
        let html: BTreeMap<String, &str> = outputs
            .iter()
            .map(|(page, html)| (Self::key(page), *html))
            .collect();
        for (key, entry) in &self.next.pages {
            let path = self.object(&entry.blob);
            if path.exists() {
                continue;
            }
            if let Some(contents) = html.get(key) {
                Self::write_object(&path, contents)?;
            }
        }
        let json = serde_json::to_vec_pretty(&self.next)
            .map_err(|e| SerializeError::new(Artifact::Cache, e))?;
        crate::fs::write(self.dir.join(MANIFEST), json)?;
        self.prune();
        Ok(())
    }

    /// Absolute path of a blob in the object store, sharded by hash prefix to
    /// keep any one directory small.
    fn object(&self, blob: &Hash) -> PathBuf {
        let hex = blob.hex();
        let (shard, _) = hex.split_at(2.min(hex.len()));
        self.dir.join(OBJECTS).join(shard).join(hex)
    }

    /// Write a blob, creating its shard directory.
    fn write_object(path: &Path, contents: &str) -> Result<()> {
        if let Some(parent) = path.parent() {
            crate::fs::create_dir_all(parent)?;
        }
        crate::fs::write(path, contents)
    }

    /// Remove object files not referenced by the next manifest. Best-effort:
    /// the cache is regenerable, so a housekeeping failure never fails a build.
    fn prune(&self) {
        let live: BTreeSet<String> = self
            .next
            .pages
            .values()
            .map(|e| e.blob.hex().to_owned())
            .collect();
        let root = self.dir.join(OBJECTS);
        let Ok(shards) = fs::read_dir(&root) else {
            return;
        };
        for shard in shards.flatten() {
            let Ok(blobs) = fs::read_dir(shard.path()) else {
                continue;
            };
            for blob in blobs.flatten() {
                let referenced = blob
                    .file_name()
                    .to_str()
                    .is_some_and(|name| live.contains(name));
                if !referenced {
                    let _ = fs::remove_file(blob.path());
                }
            }
        }
    }

    fn key(page: &Page) -> String {
        page.source.display().to_string()
    }
}
