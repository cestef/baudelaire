//! Authoritative build cache: content hashes, dependency edges, and rendered
//! output, persisted between builds to drive incremental rebuilds.
//!
//! Layout under the cache directory:
//!
//! ```text
//! manifest.json          # small: config fingerprint + per-page metadata
//! objects/ab/abcd..       # rendered HTML, content-addressed by blob hash
//! ```
//!
//! The manifest holds only metadata: hashes, dependency edges, output paths,
//! and a pointer to each page's HTML *blob*. The HTML itself lives in a
//! content-addressed object store, so a load parses a small manifest instead of
//! deserializing every page's markup, identical output is stored once, and an
//! unchanged blob is never rewritten.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::codegen::Value;
use crate::config::Config;
use crate::content::Page;
use crate::error::warning::ManifestUnreadable;
use crate::error::{Artifact, Result, SerializeError};
use crate::graph::access::{self, Root};
use crate::graph::{Deps, FileDigests, Hash, Reads};
use crate::render::ImageRef;
use crate::ui::Ui;

/// The on-disk manifest file name under the cache directory.
const MANIFEST: &str = "manifest.json";

/// Subdirectory holding content-addressed HTML blobs.
const OBJECTS: &str = "objects";

/// A page's cached compile result and the fingerprints that validate it. The
/// rendered HTML is not inlined: [`Entry::blob`] points at it in the object
/// store, read only on a cache hit.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Entry {
    /// Hash of the page's own source.
    hash: Hash,
    /// Dependency files and their hashes at compile time. Keyed by `PathBuf`
    /// (serde-serialized, not `Display`) so a non-UTF-8 path round-trips
    /// exactly instead of being lossily replaced and permanently missing.
    deps: BTreeMap<PathBuf, Hash>,
    /// Injected values the page read (`sys.inputs.baudelaire.git.hash`, ..) and
    /// their digests at compile time, so a new commit or day rebuilds only the
    /// pages that display the value that changed. `None` records a read of an
    /// absent value, so its later appearance still invalidates. Absent from
    /// pre-tracking manifests, hence `default`.
    #[serde(default)]
    meta: BTreeMap<String, Option<Hash>>,
    /// Content hash of the rendered HTML; locates its blob in the object store.
    blob: Hash,
    /// Images this page externalized out of the DOM. Re-copied into `dist` on a
    /// cache hit, since the asset directory is regenerated every build. Absent
    /// from manifests written before externalization existed, hence `default`.
    #[serde(default)]
    images: Vec<ImageRef>,
}

/// The serialized cache manifest: metadata only, no page markup.
#[derive(Debug, Default, Serialize, Deserialize)]
struct Manifest {
    /// Fingerprint of the site-wide inputs that produced these entries (config,
    /// asset map, link map, embedded assets). Any change invalidates the whole
    /// manifest, since it can alter every permalink or embedded input. Build metadata
    /// is *not* here: it's tracked per page via [`Entry::meta`].
    config: Option<Hash>,
    /// Entries keyed by page source path.
    pages: BTreeMap<PathBuf, Entry>,
}

/// The render-side inputs folded into the cache fingerprint alongside the
/// config: the processed-asset URL map, the page-to-permalink map, and the
/// embedded-asset content hash. None are visible to the per-page
/// dependency tracker (asset renames and link resolution happen in the render
/// pass; embeds inline bytes typst never reads), so they are fingerprinted
/// whole: any change invalidates every page.
#[derive(std::hash::Hash)]
pub struct RenderInputs {
    pub assets: Hash,
    pub links: Hash,
    /// The responsive width-variant manifest: a page's `srcset` markup changes
    /// when its images' variants do.
    pub srcsets: Hash,
    /// Present only when `embed` is on: a content hash of the inlined assets.
    pub embeds: Option<Hash>,
}

/// The build cache. Loads the previous manifest, answers reuse queries, and
/// accumulates the next manifest as pages are reused or recompiled.
pub struct Cache {
    dir: PathBuf,
    enabled: bool,
    config: Hash,
    prev: Manifest,
    next: Manifest,
    /// Per-build file-hash memo: a dependency shared by many pages (a template,
    /// a theme module) is hashed once across validation and recording, not once
    /// per page.
    digests: FileDigests,
    /// The tracked injected values (base path + current tree), for resolving the
    /// digest of a page's recorded value reads. Owned so the cache is
    /// self-contained across the build.
    roots: Vec<(String, Value)>,
}

impl Cache {
    /// Load the cache for a build. When incremental builds are disabled the
    /// cache still records the next manifest but never reports a hit.
    ///
    /// The manifest fingerprint mixes the config, the asset map, the link map,
    /// and (when `embed` is on) the embedded asset contents: the site-wide
    /// inputs that can alter any page. Build metadata (a new commit or day) is
    /// deliberately *not* here: it's tracked per page against `roots`, so it
    /// rebuilds only the pages that display the value that changed. Only the
    /// small manifest is read here; HTML blobs are fetched lazily on a hit.
    pub fn load(
        config: &Config,
        render: &RenderInputs,
        roots: Vec<(String, Value)>,
        ui: &Ui,
    ) -> Result<Self> {
        let dir = config.cache.dir.clone();
        let manifest = dir.join(MANIFEST);
        let prev = match fs::read(&manifest) {
            // a present-but-unparseable manifest (torn write, corruption, manual
            // edit) isn't a fresh cache: warn and rebuild rather than silently
            // treat it as "no cache".
            Ok(bytes) => match serde_json::from_slice(&bytes) {
                Ok(prev) => prev,
                Err(e) => {
                    ui.warn(ManifestUnreadable {
                        path: manifest.clone(),
                        source: e,
                    });
                    Manifest::default()
                }
            },
            // absent manifest is the normal first-build case, stay silent.
            Err(_) => Manifest::default(),
        };
        let fingerprint = Hash::of(&(config, render));
        Ok(Self {
            dir,
            enabled: config.cache.incremental,
            next: Manifest {
                config: Some(fingerprint),
                pages: BTreeMap::new(),
            },
            config: fingerprint,
            prev,
            digests: FileDigests::default(),
            roots,
        })
    }

    /// Borrow the tracked roots for a value-digest resolution.
    fn roots(&self) -> Vec<Root<'_>> {
        self.roots
            .iter()
            .map(|(base, tree)| Root { base, tree })
            .collect()
    }

    /// Cached HTML for `page` if still valid: its content fingerprint, every
    /// dependency, and the manifest fingerprint are all unchanged, and its blob
    /// is still present in the object store. A hit carries the entry into the
    /// next manifest so it survives to the following build.
    ///
    /// `fingerprint` hashes the exact text typst compiles, so it validates
    /// generated pages (taxonomies, paginated indexes) too, whose synthetic
    /// sources never touch disk and so have no file to hash.
    pub fn reuse(&mut self, page: &Page, fingerprint: &Hash) -> Option<(String, Vec<ImageRef>)> {
        if !self.enabled || self.prev.config.as_ref() != Some(&self.config) {
            return None;
        }
        let entry = self.prev.pages.get(Self::key(page))?;
        if &entry.hash != fingerprint {
            return None;
        }
        if !entry
            .deps
            .iter()
            .all(|(path, hash)| self.digests.of(path).as_ref() == Some(hash))
        {
            return None;
        }
        // every injected value the page read must still hash the same, so a
        // commit or day that changes a value it displays is a miss, and one that
        // doesn't is a hit.
        let roots = self.roots();
        if !entry
            .meta
            .iter()
            .all(|(key, hash)| access::digest(&roots, key) == *hash)
        {
            return None;
        }
        let html = fs::read_to_string(self.object(&entry.blob)).ok()?;
        // verify the blob against its content address: a torn write leaves bytes
        // not matching the name that claims their hash. mismatch is a miss.
        if Hash::of_bytes(html.as_bytes()) != entry.blob {
            return None;
        }
        let images = entry.images.clone();
        self.next
            .pages
            .insert(Self::key(page).to_path_buf(), entry.clone());
        Some((html, images))
    }

    /// Record a freshly compiled page against its content fingerprint, its
    /// dependency hashes, and the digests of the injected values it read,
    /// staging its HTML for the object store.
    pub fn record(
        &mut self,
        page: &Page,
        fingerprint: Hash,
        html: &str,
        deps: &Deps,
        reads: &Reads,
        images: &[ImageRef],
    ) {
        let meta = access::digests(&self.roots(), reads);
        let deps = deps
            .files()
            .iter()
            .filter_map(|p| Some((p.clone(), self.digests.of(p)?)))
            .collect();
        let blob = Hash::of_bytes(html.as_bytes());
        self.next.pages.insert(
            Self::key(page).to_path_buf(),
            Entry {
                hash: fingerprint,
                deps,
                meta,
                blob,
                images: images.to_vec(),
            },
        );
    }

    /// Persist the manifest and every referenced HTML blob, then drop objects no
    /// longer referenced. Blobs are content-addressed and written write-once, so
    /// an unchanged page's markup is never rewritten. `outputs` supplies the HTML
    /// for freshly recorded pages (cache hits already have their blob on disk).
    pub fn save(&self, outputs: &[(&Page, &str)]) -> Result<()> {
        crate::fs::create_dir_all(&self.dir)?;
        let html: BTreeMap<&Path, &str> = outputs
            .iter()
            .map(|(page, html)| (Self::key(page), *html))
            .collect();
        // collect blobs needing a write (new content only), keyed by path so
        // two pages sharing identical markup stage one write (a duplicate
        // would race itself in the parallel pass), then write them in
        // parallel: independent content-addressed files.
        let pending: BTreeMap<PathBuf, &str> = self
            .next
            .pages
            .iter()
            .filter_map(|(key, entry)| {
                let path = self.object(&entry.blob);
                if path.exists() {
                    return None;
                }
                html.get(key.as_path()).map(|contents| (path, *contents))
            })
            .collect();
        pending
            .par_iter()
            .try_for_each(|(path, contents)| Self::write_object(path, contents))?;
        let json = serde_json::to_vec_pretty(&self.next)
            .map_err(|e| SerializeError::new(Artifact::Cache, e))?;
        Self::write_atomic(&self.dir.join(MANIFEST), json.as_slice())?;
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

    /// Write a blob, creating its shard directory. Atomic, so a crash can never
    /// leave a truncated blob whose name claims a hash its bytes don't have.
    fn write_object(path: &Path, contents: &str) -> Result<()> {
        if let Some(parent) = path.parent() {
            crate::fs::create_dir_all(parent)?;
        }
        Self::write_atomic(path, contents.as_bytes())
    }

    /// Write `bytes` to `path` via a temporary sibling and a rename, so a reader
    /// only ever sees the complete file (rename is atomic on the same
    /// filesystem). Content-addressed blobs are written once per build, so the
    /// fixed `.tmp` suffix never races itself.
    fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
        let tmp = path.with_extension("tmp");
        crate::fs::write(&tmp, bytes)?;
        crate::fs::rename(&tmp, path)
    }

    /// Remove object files not referenced by the next manifest. Best-effort:
    /// the cache is regenerable, so a housekeeping failure never fails a build.
    fn prune(&self) {
        let live: BTreeSet<String> = self.next.pages.values().map(|e| e.blob.hex()).collect();
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

    fn key(page: &Page) -> &Path {
        &page.source
    }
}
