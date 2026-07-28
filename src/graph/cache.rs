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
use crate::content::{Data, Page};
use crate::error::warning::ManifestUnreadable;
use crate::error::{Artifact, Result, SerializeError};
use crate::graph::access::{self, Root};
use crate::graph::{Deps, FileDigests, Hash, Reads, Renderer};
use crate::render::{Fragments, ImageRef};
use crate::ui::Ui;

/// The on-disk manifest file name under the cache directory.
const MANIFEST: &str = "manifest.json";

/// Subdirectory holding content-addressed HTML blobs.
const OBJECTS: &str = "objects";

/// Manifest key prefix reserved for generated listings, which have no real
/// source file. Not a valid relative path under the project root, so it cannot
/// collide with a real page's key.
const GENERATED: &str = "<generated>";

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
    ///
    /// `None` records a dependency that could not be hashed, so its later
    /// appearance still invalidates; dropping it would leave it unchecked.
    deps: BTreeMap<PathBuf, Option<Hash>>,
    /// Injected values the page read (`sys.inputs.baudelaire.git.hash`, ..) and
    /// their digests at compile time, so a new commit or day rebuilds only the
    /// pages that display the value that changed. `None` records a read of an
    /// absent value, so its later appearance still invalidates. Absent from
    /// pre-tracking manifests, hence `default`.
    #[serde(default)]
    meta: BTreeMap<String, Option<Hash>>,
    /// Content hash of the rendered HTML; locates its blob in the object store.
    blob: Hash,
    /// What the render pass produced besides the markup, replayed on a hit.
    /// Defaulted so a manifest written by an older layout still *parses*: the
    /// schema in the fingerprint (see [`crate::graph::Renderer`]) is what
    /// decides it is unusable, and that path rebuilds silently instead of
    /// warning every user once per upgrade.
    #[serde(default)]
    outputs: Outputs,
}

/// The render-side results of compiling a page, stored alongside its markup
/// because nothing here can be recovered from the markup afterwards.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Outputs {
    /// Images the page externalized out of the DOM. Re-copied into `dist` on a
    /// cache hit, since the asset directory is regenerated every build.
    pub images: Vec<ImageRef>,
    /// Raw targets of the broken internal links the page produced.
    ///
    /// Stored so the link check sees a cached page too. Feeding it only
    /// freshly-compiled pages meant a second build reported nothing and
    /// `links { strict #true }` *passed*: a gate that silently weakened on
    /// rebuild.
    pub broken: Vec<String>,
    /// The page's head and body markup, captured only while the single-file
    /// export is on. Stored because it cannot be recovered from the rendered
    /// page afterwards without parsing it, and a cache-served page has to be
    /// bundled just like a freshly compiled one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fragments: Option<Fragments>,
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
    /// The project root, so every path stored in the manifest can be recorded
    /// relative to it.
    root: PathBuf,
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
    /// Blobs found on disk not matching their own content address, to be
    /// rewritten by [`Cache::save`] rather than left broken forever.
    corrupt: std::collections::HashSet<Hash>,
}

impl Cache {
    /// Load the cache for a build. When incremental builds are disabled the
    /// cache still records the next manifest but never reports a hit, and
    /// fingerprints it identically to a normal build's, so `--no-cache` costs
    /// one cold build rather than poisoning the next.
    ///
    /// The manifest fingerprint mixes the config, the renderer's own identity,
    /// the asset map, the link map,
    /// and (when `embed` is on) the embedded asset contents: the site-wide
    /// inputs that can alter any page. Build metadata (a new commit or day) is
    /// deliberately *not* here: it's tracked per page against `roots`, so it
    /// rebuilds only the pages that display the value that changed. Only the
    /// small manifest is read here; HTML blobs are fetched lazily on a hit.
    pub fn load(
        config: &Config,
        render: &RenderInputs,
        roots: Vec<(String, Value)>,
        root: &Path,
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
        let fingerprint = Hash::of(&(config, render, Renderer::current()));
        Ok(Self {
            dir,
            root: crate::fs::canonical(root),
            enabled: config.cache.incremental,
            next: Manifest {
                config: Some(fingerprint),
                pages: BTreeMap::new(),
            },
            config: fingerprint,
            prev,
            digests: FileDigests::default(),
            roots,
            corrupt: std::collections::HashSet::new(),
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
    pub fn reuse(&mut self, page: &Page, fingerprint: &Hash) -> Option<(String, Outputs)> {
        if !self.enabled || self.prev.config.as_ref() != Some(&self.config) {
            return None;
        }
        let key = self.key(page);
        let entry = self.prev.pages.get(&key)?;
        if &entry.hash != fingerprint {
            return None;
        }
        if !entry
            .deps
            .iter()
            .all(|(path, hash)| self.digests.of(&self.resolve(path)) == *hash)
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
        // not matching the name that claims their hash. mismatch is a miss, and
        // the bad file is remembered so `save` overwrites it: it is still
        // referenced (so the prune keeps it) and its path already exists (so a
        // write would be skipped), which left that page missing on every build
        // from then on with no way to self-heal.
        if Hash::of_bytes(html.as_bytes()) != entry.blob {
            self.corrupt.insert(entry.blob);
            return None;
        }
        let entry = entry.clone();
        let outputs = entry.outputs.clone();
        self.next.pages.insert(key, entry);
        Some((html, outputs))
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
        outputs: &Outputs,
    ) {
        let meta = access::digests(&self.roots(), reads);
        let deps = deps
            .files()
            .iter()
            .map(|p| (self.portable(p), self.digests.of(p)))
            .collect();
        let blob = Hash::of_bytes(html.as_bytes());
        self.next.pages.insert(
            self.key(page),
            Entry {
                hash: fingerprint,
                deps,
                meta,
                blob,
                outputs: outputs.clone(),
            },
        );
    }

    /// Persist the manifest and every referenced HTML blob, then drop objects no
    /// longer referenced. Blobs are content-addressed and written write-once, so
    /// an unchanged page's markup is never rewritten. `outputs` supplies the HTML
    /// for freshly recorded pages (cache hits already have their blob on disk).
    pub fn save<'a>(&self, outputs: impl IntoIterator<Item = (&'a Page, &'a str)>) -> Result<()> {
        crate::fs::create_dir_all(&self.dir)?;
        let html: BTreeMap<PathBuf, &str> = outputs
            .into_iter()
            .map(|(page, html)| (self.key(page), html))
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
                if path.exists() && !self.corrupt.contains(&entry.blob) {
                    return None;
                }
                html.get(key).map(|contents| (path, *contents))
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

    /// The manifest key for a page.
    ///
    /// Portable (see [`Cache::portable`]), and generated listings sit under a
    /// reserved prefix: their source path is fabricated, so a real page written
    /// at the same path would share one entry with the listing, overwrite it
    /// every build, and leave both missing forever.
    fn key(&self, page: &Page) -> PathBuf {
        let path = self.portable(&page.source);
        match page.data {
            Data::Generated(_) => Path::new(GENERATED).join(path),
            Data::Export | Data::Empty => path,
        }
    }

    /// A path as the manifest stores it: relative to the project root when it
    /// lies inside it (so a warm cache survives the site moving), unchanged
    /// otherwise — the typst package cache is machine-global anyway.
    fn portable(&self, path: &Path) -> PathBuf {
        let absolute = crate::fs::canonical(path);
        absolute
            .strip_prefix(&self.root)
            .unwrap_or(&absolute)
            .to_path_buf()
    }

    /// The inverse of [`Cache::portable`]: a stored key back to a real path.
    fn resolve(&self, path: &Path) -> PathBuf {
        self.root.join(path)
    }
}

#[cfg(test)]
mod tests {
    use super::{Cache, GENERATED, RenderInputs};
    use crate::config::Config;
    use crate::content::{Data, Frontmatter, Page, PageId, Siblings};
    use crate::graph::Hash;
    use crate::ui::{Level, Ui};
    use std::path::{Path, PathBuf};

    fn page(source: &str, data: Data) -> Page {
        Page {
            id: PageId::new("posts", source),
            source: PathBuf::from(source),
            frontmatter: Frontmatter::default(),
            body: String::new(),
            data,
            collection: "posts".into(),
            permalink: "/p/".into(),
            output: PathBuf::new(),
            template: None,
            lang: "en".into(),
            siblings: Siblings::default(),
            translations: Vec::new(),
        }
    }

    fn cache(root: &Path) -> Cache {
        let mut config = Config::default();
        config.cache.dir = root.join(".cache");
        let render = RenderInputs {
            assets: Hash::of_bytes(b""),
            links: Hash::of_bytes(b""),
            srcsets: Hash::of_bytes(b""),
            embeds: None,
        };
        Cache::load(&config, &render, Vec::new(), root, &Ui::new(Level::Silent)).expect("cache")
    }

    /// A generated listing fabricates a source path that never exists on disk.
    /// Sharing one manifest entry with a real page at that path made the two
    /// overwrite each other every build, so both missed forever.
    #[test]
    fn a_generated_listing_cannot_collide_with_a_real_page() {
        let tmp = tempfile::tempdir().unwrap();
        let cache = cache(tmp.path());
        let path = tmp.path().join("content/tags/rust.typ");
        let path = path.to_str().expect("utf-8 tempdir");

        let real = cache.key(&page(path, Data::Empty));
        let listing = cache.key(&page(path, Data::Generated(String::new())));

        assert_ne!(real, listing);
        assert!(listing.starts_with(GENERATED), "{listing:?}");
    }

    /// Manifest keys are relative to the project root, so a warm cache still
    /// matches after the site moves on disk.
    #[test]
    fn keys_are_relative_to_the_project_root() {
        let tmp = tempfile::tempdir().unwrap();
        let cache = cache(tmp.path());
        let path = tmp.path().join("content/posts/a.typ");

        let key = cache.key(&page(path.to_str().expect("utf-8 tempdir"), Data::Empty));

        assert_eq!(key, Path::new("content/posts/a.typ"));
    }
}
