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
//! content-addressed object store ([`super::objects`]), so a load parses a small
//! manifest instead of deserializing every page's markup, identical output is
//! stored once, and an unchanged blob is never rewritten.

use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::codegen::Value;
use crate::config::Config;
use crate::content::{Data, Page};
use crate::error::warning::ManifestUnreadable;
use crate::error::{Artifact, Result, SerializeError};
use crate::graph::access::{Root, Roots};
use crate::graph::objects::Objects;
use crate::graph::{Deps, FileDigests, Hash, Reads, Renderer};
use crate::render::{
    AssetDeps, Finding, Fragments, ImageRef, Inline, LinkDeps, Outbound, RenderMaps, SrcSetDeps,
    Syndicated, Target, UrlDeps, Weight,
};
use crate::ui::Ui;

/// The on-disk manifest file name under the cache directory.
const MANIFEST: &str = "manifest.json";

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
    /// The permalinks this page's links resolved against, keyed by the target's
    /// source path, so a page rebuilds when a page it links to moves and stays
    /// cached when an unrelated one does.
    ///
    /// `None` records a link that resolved to no page, so a page later
    /// appearing at that path still invalidates; dropping it would leave a
    /// broken link permanently broken in cached markup. Absent from
    /// pre-tracking manifests, hence `default`.
    #[serde(default)]
    links: LinkDeps,
    /// The URLs this page's already-URL links named, and whether a page sat
    /// there, so a page appearing at a URL something already linked to rebuilds
    /// the linker and gains its backlink.
    ///
    /// `false` is the load-bearing half: a link to a page that does not exist
    /// yet leaves no other trace on the linking page, so without it the page can
    /// be added and every linker stays a hit, replaying an outbound edge it
    /// never recorded. Absent from pre-tracking manifests, hence `default`.
    #[serde(default)]
    urls: UrlDeps,
    /// The responsive variants this page's images matched, keyed by the source
    /// path each `<img>` named, so regenerating one image's variants rebuilds
    /// the pages showing it and leaves the rest cached.
    ///
    /// `None` records a source with no variants, so an image that gains some
    /// later still invalidates the page displaying it.
    #[serde(default)]
    srcsets: SrcSetDeps,
    /// The processed-asset URLs this page's references resolved to, keyed by the
    /// request path each named, so re-fingerprinting one asset rebuilds the
    /// pages that reference it and leaves the rest cached.
    ///
    /// `None` records a reference to an asset that was not there, so one
    /// appearing later still invalidates the page pointing at it. Only paths
    /// under the asset prefix are recorded: nothing else can ever be a key.
    #[serde(default)]
    assets: AssetDeps,
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
    /// The heading ids this page exposes, and the links it carries into a
    /// section of another page.
    ///
    /// Stored for the same reason as `broken`: the deep-link check has to see a
    /// cached page too, or a second build reports nothing and the gate silently
    /// weakens on rebuild. Unlike `broken` these are the check's *inputs*, not
    /// its verdict, and the check re-runs site-wide every build: page A's
    /// verdict depends on page B's headings, so caching the verdict would go
    /// stale the moment B was edited and A was not.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub anchors: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub deep: Vec<Target>,
    /// The pages this page's own content links to.
    ///
    /// Stored for the same reason as `anchors` and `deep`: the link graph is
    /// assembled site-wide from every page, cache hits included, and a page
    /// absent from it is a page whose links silently stop being backlinks of
    /// anything the moment it is served from cache.
    #[serde(default, skip_serializing_if = "Outbound::is_empty")]
    pub outbound: Outbound,
    /// The digest of the backlinks this page was compiled with, `None` when it
    /// was compiled with none at all (`links { backlinks }` off).
    ///
    /// The page's *second-stage* input, and the only one not folded into the
    /// fingerprint the cache splits on: the graph it comes from does not exist
    /// until every page has rendered, so a page is compiled against a predicted
    /// value and this is what the repair pass checks the real one against. A
    /// page whose digest still matches is left alone, cached or freshly
    /// compiled alike.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backlinks: Option<Hash>,
    /// The page's head and body markup, captured only while the single-file
    /// export is on. Stored because it cannot be recovered from the rendered
    /// page afterwards without parsing it, and a cache-served page has to be
    /// bundled just like a freshly compiled one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fragments: Option<Fragments>,
    /// The page's prose as a full-content feed publishes it, captured only while
    /// `generate { feed { content "full" } }` is on.
    ///
    /// Stored for the same reason as `fragments`: the URLs in it are absolute
    /// and the site's chrome is gone, neither of which can be recovered from the
    /// rendered page without parsing it, and a cache-served page has to appear
    /// in the feed just like a freshly compiled one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub syndicated: Option<Syndicated>,
    /// What the lint pass found on the page, and what the page ships.
    ///
    /// Stored for the same reason as `broken`: both checks run site-wide over
    /// every page, cached ones included, and a gate that reports nothing on the
    /// second build is not a gate. The findings are a verdict about the page
    /// alone and are safe to replay; the weight is deliberately *not* one,
    /// listing what the page loads and leaving the sizes to be resolved against
    /// the build that emitted them.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub lints: Vec<Finding>,
    #[serde(default, skip_serializing_if = "Weight::is_empty")]
    pub weight: Weight,
    /// The digests of the page's inline scripts and styles, for the generated
    /// content security policy.
    ///
    /// Stored because the policy is assembled site-wide from every page's
    /// digests, cache hits included: a page absent from that union is a page
    /// whose own inline script the browser refuses to run.
    #[serde(default, skip_serializing_if = "Inline::is_empty")]
    pub inline: Inline,
}

/// The serialized cache manifest: metadata only, no page markup.
#[derive(Debug, Default, Serialize, Deserialize)]
struct Manifest {
    /// Fingerprint of the site-wide inputs that produced these entries (config,
    /// asset map, embedded assets). Any change invalidates the whole manifest,
    /// since it can alter every permalink or embedded input. Two things are
    /// deliberately *not* here, because each is tracked per page and so
    /// invalidates only the pages it reaches: build metadata, via
    /// [`Entry::meta`], and the page-to-permalink map, via [`Entry::links`].
    config: Option<Hash>,
    /// Entries keyed by page source path.
    pages: BTreeMap<PathBuf, Entry>,
    /// Entries for the artifacts compiled from *many* pages (a bundled PDF),
    /// keyed by the artifact's id. A page's entry cannot stand in for one: the
    /// document belongs to no single page and has to rebuild when any of them
    /// moves.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    bundles: BTreeMap<String, Compiled>,
}

/// What validating any compiled artifact needs: the fingerprint of the exact
/// text typst compiled, and the hash of every file that compile read.
///
/// [`Entry`] carries the same pair for a page, alongside everything a page
/// additionally records. Split out so the two are checked by one function
/// rather than by two that can drift.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Compiled {
    /// Hash of the exact text typst compiled.
    hash: Hash,
    /// Dependency files and their hashes at compile time, `None` for one that
    /// could not be hashed, exactly as [`Entry::deps`] records them.
    deps: BTreeMap<PathBuf, Option<Hash>>,
}

/// What a build reads that no page records reading, and so is fingerprinted
/// whole: nothing narrows either of these to the pages it affects.
///
/// Everything that used to sit here is now tracked per page and so invalidates
/// only the pages it reaches: link resolution via [`Entry::links`], the
/// responsive variant manifest via [`Entry::srcsets`], the processed-asset URL
/// map via [`Entry::assets`], and the social card and every inlined or embedded
/// file through the page's own [`Entry::deps`]. What is left over is what typst
/// itself cannot see: a module served from memory has no path, and a font is
/// resolved by name out of a store rather than opened as a file.
#[derive(std::hash::Hash)]
pub struct SiteInputs {
    /// A content hash of the generated `@baudelaire/*` Typst modules.
    ///
    /// A page *does* import these, but they exist only in memory, so they
    /// resolve to no path and never reach the dependency tracker. They carry
    /// nothing volatile by construction, so hashing them whole costs a full
    /// rebuild only when baudelaire or the site's identity changes.
    pub modules: Hash,
    /// A content hash of the fonts the site ships, or `None` when it ships none.
    ///
    /// A page resolves a face by name out of a store built by walking a
    /// directory, so no font file is ever read through the world and none can
    /// appear in a page's dependency set. See
    /// [`Fonts::digest`](crate::world::Project::fonts).
    pub fonts: Option<Hash>,
}

/// One freshly compiled page as [`Cache::record`] takes it: the text it was
/// built from, everything it depended on while building, and what came out.
///
/// A parameter object rather than a row of positional arguments, so a new kind
/// of dependency is one field here and one line at the call site, instead of a
/// wider signature every caller has to re-spell.
#[derive(Clone, Copy)]
pub struct Recorded<'a> {
    pub page: &'a Page,
    /// Hash of the exact text typst compiled.
    pub fingerprint: Hash,
    pub html: &'a str,
    /// The files the compile read.
    pub deps: &'a Deps,
    /// The injected values the page read.
    pub reads: &'a Reads,
    /// The permalinks the page's links resolved against.
    pub links: &'a LinkDeps,
    /// The URLs the page's already-URL links named, and whether one was served.
    pub urls: &'a UrlDeps,
    /// The responsive variants the page's images matched.
    pub srcsets: &'a SrcSetDeps,
    /// The processed-asset URLs the page's references resolved to.
    pub assets: &'a AssetDeps,
    /// What the render pass produced besides the markup.
    pub outputs: &'a Outputs,
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
    /// This build's page-to-permalink map, keyed the way the manifest stores
    /// paths, to revalidate each page's recorded [`Entry::links`].
    links: BTreeMap<PathBuf, String>,
    /// This build's variant digests, to revalidate each page's recorded
    /// [`Entry::srcsets`]. Digested by [`SrcSets`] itself so the recorded and
    /// the current value cannot be computed two different ways.
    srcsets: BTreeMap<String, Hash>,
    /// This build's request-to-served asset URLs, to revalidate each page's
    /// recorded [`Entry::assets`].
    assets: BTreeMap<String, String>,
    /// Every URL this build serves a page at, to revalidate each page's recorded
    /// [`Entry::urls`].
    urls: HashSet<String>,
    /// The content-addressed store holding every page's rendered markup.
    objects: Objects,
}

impl Cache {
    /// Load the cache for a build. When incremental builds are disabled the
    /// cache still records the next manifest but never reports a hit, and
    /// fingerprints it identically to a normal build's, so `--no-cache` costs
    /// one cold build rather than poisoning the next.
    ///
    /// The manifest fingerprint mixes the config, the renderer's own identity,
    /// the asset map, and (when `embed` is on) the embedded asset contents: the
    /// site-wide inputs that can alter any page. Two things are deliberately
    /// *not* in it, because both are tracked per page instead and so rebuild
    /// only the pages they actually affect: build metadata (a new commit or
    /// day), validated against `roots`, and the page-to-permalink map,
    /// validated against `links`. Only the small manifest is read here; HTML
    /// blobs are fetched lazily on a hit.
    pub fn load(
        config: &Config,
        inputs: &SiteInputs,
        roots: Vec<(String, Value)>,
        maps: RenderMaps<'_>,
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
                        path: manifest,
                        source: e,
                    });
                    Manifest::default()
                }
            },
            // absent manifest is the normal first-build case, stay silent.
            Err(_) => Manifest::default(),
        };
        let fingerprint = Hash::of(&(config, inputs, Renderer::current()));
        let root = crate::fs::canonical(root);
        Ok(Self {
            objects: Objects::new(&dir),
            dir,
            enabled: config.cache.incremental,
            next: Manifest {
                config: Some(fingerprint),
                ..Manifest::default()
            },
            config: fingerprint,
            prev,
            digests: FileDigests::default(),
            roots,
            links: maps
                .links
                .entries()
                .map(|(source, permalink)| (Self::portable(&root, source), permalink.to_owned()))
                .collect(),
            srcsets: maps.srcsets.digests(),
            assets: maps.assets.served().clone(),
            urls: maps.links.urls().clone(),
            root,
        })
    }

    /// Every file this build recorded reading, as absolute paths under the
    /// project root: sources, their transitive imports, and the data files a
    /// page loaded.
    ///
    /// For the dev server, which otherwise watches four fixed trees and asks the
    /// site to name anything else in `serve { include }`. The build already knows
    /// (that is how a page whose data file changed recompiles), and repeating
    /// the knowledge in config is how an edit to `data/authors.yaml` silently
    /// does nothing. Paths recorded as absolute are left out: those are outside
    /// the project (typst's package cache), and nothing there is edited by hand.
    pub fn read(&self) -> impl Iterator<Item = PathBuf> + '_ {
        self.next
            .pages
            .values()
            .flat_map(|entry| entry.deps.keys())
            .filter(|dep| dep.is_relative())
            .map(|dep| self.root.join(dep))
    }

    /// Borrow the tracked roots for a value-digest resolution.
    fn roots(&self) -> Roots<'_> {
        self.roots.iter().map(Root::from).collect()
    }

    /// The links the last build recorded for `page`, if it has seen it.
    ///
    /// What this build's backlinks are predicted from before it has rendered
    /// anything: never trusted, since what a page is actually compiled with is
    /// checked against the graph this build produces and the pages that disagree
    /// are compiled again (see `engine::links::Graph`). Being one build behind is
    /// the point, because an edit that changes no links leaves it exact, which
    /// is nearly every edit.
    pub fn recorded(&self, page: &Page) -> Option<&Outbound> {
        Some(&self.prev.pages.get(&self.key(page))?.outputs.outbound)
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
        if !self.intact(&entry.deps) {
            return None;
        }
        // every injected value the page read must still hash the same, so a
        // commit or day that changes a value it displays is a miss, and one that
        // doesn't is a hit.
        let roots = self.roots();
        if !entry
            .meta
            .iter()
            .all(|(key, hash)| roots.digest(key) == *hash)
        {
            return None;
        }
        // every page this one linked to must still sit at the same URL, and
        // every link that resolved to nothing must still resolve to nothing, so
        // a page that moved rebuilds its linkers and no one else.
        if !entry
            .links
            .iter()
            .all(|(path, permalink)| self.links.get(path) == permalink.as_ref())
        {
            return None;
        }
        // every URL this page linked to by name must still be served by a page,
        // and every one that named nothing must still name nothing, so a page
        // that appears at a linked-to URL rebuilds its linkers.
        if !entry
            .urls
            .iter()
            .all(|(url, served)| self.urls.contains(url) == *served)
        {
            return None;
        }
        // every image this page showed must still resolve to the same variants,
        // and one that had none must still have none.
        if !entry
            .srcsets
            .iter()
            .all(|(source, digest)| self.srcsets.get(source) == digest.as_ref())
        {
            return None;
        }
        // every asset this page referenced must still be served from the same
        // URL, and one that was absent must still be absent.
        if !entry
            .assets
            .iter()
            .all(|(request, served)| self.assets.get(request) == served.as_ref())
        {
            return None;
        }
        let entry = entry.clone();
        let html = self.objects.read(&entry.blob)?;
        let outputs = entry.outputs.clone();
        self.next.pages.insert(key, entry);
        Some((html, outputs))
    }

    /// Whether every file a compile read still hashes to what it hashed then.
    ///
    /// One check, shared by every artifact the cache validates: a page and a
    /// bundled document ask the same question of the same recorded shape, and
    /// two spellings of it would drift.
    fn intact(&self, deps: &BTreeMap<PathBuf, Option<Hash>>) -> bool {
        deps.iter()
            .all(|(path, hash)| self.digests.of(&self.resolve(path)) == *hash)
    }

    /// The files a compile read, hashed and keyed the portable way the manifest
    /// stores them. The counterpart of [`Cache::intact`], and the reason the
    /// two agree about what a recorded dependency looks like.
    fn digested(&self, deps: &Deps) -> BTreeMap<PathBuf, Option<Hash>> {
        deps.files()
            .iter()
            .map(|p| (Self::portable(&self.root, p), self.digests.of(p)))
            .collect()
    }

    /// Whether a bundled document compiled from `fingerprint` can be left as it
    /// is: the same module text, every file it read unchanged, and the file
    /// still on disk.
    ///
    /// The disk check is not belt-and-braces. Unlike a page's HTML, which is
    /// rewritten from the object store every build, a bundle is written only by
    /// the build that compiles it, so a `dist` cleared behind the cache's back
    /// would leave it missing forever.
    pub fn reuse_bundle(&mut self, id: &str, fingerprint: &Hash, path: &Path) -> bool {
        let hit = self.enabled
            && self.prev.config.as_ref() == Some(&self.config)
            && path.exists()
            && match self.prev.bundles.get(id) {
                Some(entry) => &entry.hash == fingerprint && self.intact(&entry.deps),
                None => false,
            };
        if hit && let Some(entry) = self.prev.bundles.get(id).cloned() {
            self.next.bundles.insert(id.to_owned(), entry);
        }
        hit
    }

    /// Record a freshly compiled bundle, so the next build can leave it alone.
    pub fn record_bundle(&mut self, id: &str, fingerprint: Hash, deps: &Deps) {
        let deps = self.digested(deps);
        self.next.bundles.insert(
            id.to_owned(),
            Compiled {
                hash: fingerprint,
                deps,
            },
        );
    }

    /// Record a freshly compiled page: its content fingerprint, its dependency
    /// hashes, the digests of the injected values it read, and the permalinks
    /// its links resolved against, staging its HTML for the object store.
    pub fn record(&mut self, compiled: Recorded<'_>) {
        let Recorded {
            page,
            fingerprint,
            html,
            deps,
            reads,
            links,
            urls,
            srcsets,
            assets,
            outputs,
        } = compiled;
        let meta = self.roots().digests(reads);
        let deps = self.digested(deps);
        let links = links
            .iter()
            .map(|(path, permalink)| (Self::portable(&self.root, path), permalink.clone()))
            .collect();
        let blob = Hash::of_bytes(html.as_bytes());
        self.next.pages.insert(
            self.key(page),
            Entry {
                hash: fingerprint,
                deps,
                meta,
                links,
                urls: urls.clone(),
                srcsets: srcsets.clone(),
                assets: assets.clone(),
                blob,
                outputs: outputs.clone(),
            },
        );
    }

    /// Record a page compiled again for its backlinks alone, keeping the
    /// dependencies its *first* compile recorded.
    ///
    /// A repair draws no sidecars, so nothing it saw includes the card or PDF
    /// template that compile read, and those are inputs to the page's output all
    /// the same (the compile folds them in for exactly that reason).
    /// Recording the repair the ordinary way narrowed the entry to the HTML
    /// compile's own files: editing a card helper then changed no hash this
    /// cache checks, the page stayed a hit, and `dist` kept serving the image
    /// the old helper drew. The union is what the page depends on, since a
    /// repair reads a subset of what the full compile did.
    pub fn relink(&mut self, compiled: Recorded<'_>) {
        let key = self.key(compiled.page);
        let inherited = self
            .next
            .pages
            .get(&key)
            .map(|entry| entry.deps.clone())
            .unwrap_or_default();
        self.record(compiled);
        if let Some(entry) = self.next.pages.get_mut(&key) {
            // The fresh hashes win where both saw a file: same build, same
            // digests, but the repair's are the ones it actually read.
            let fresh = std::mem::replace(&mut entry.deps, inherited);
            entry.deps.extend(fresh);
        }
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
        // Only freshly recorded pages carry markup to write; a cache hit's blob
        // is already stored under the same address.
        let blobs = self
            .next
            .pages
            .iter()
            .filter_map(|(key, entry)| Some((&entry.blob, *html.get(key)?)));
        self.objects.write(blobs)?;
        let json = serde_json::to_vec_pretty(&self.next)
            .map_err(|e| SerializeError::new(Artifact::Cache, e))?;
        Objects::atomic(&self.dir.join(MANIFEST), json.as_slice())?;
        self.objects.prune(&self.live());
        Ok(())
    }

    /// The blobs the next manifest still references; everything else in the
    /// object store is garbage.
    fn live(&self) -> HashSet<Hash> {
        self.next.pages.values().map(|entry| entry.blob).collect()
    }

    /// The manifest key for a page.
    ///
    /// Portable (see [`Cache::portable`]), and generated listings sit under a
    /// reserved prefix: their source path is fabricated, so a real page written
    /// at the same path would share one entry with the listing, overwrite it
    /// every build, and leave both missing forever.
    fn key(&self, page: &Page) -> PathBuf {
        let path = Self::portable(&self.root, &page.source);
        match page.data {
            Data::Generated { .. } => Path::new(GENERATED).join(path),
            // A markdown page has a real file, so it keys by it like any other.
            #[cfg(feature = "markdown")]
            Data::Lowered { .. } => path,
            Data::Export | Data::Empty => path,
        }
    }

    /// A path as the manifest stores it: relative to the project root when it
    /// lies under it (so a warm cache survives the site moving), absolute
    /// otherwise — the typst package cache is machine-global anyway. Never
    /// relative to the process's working directory, and never dependent on
    /// whether the file exists yet: one path has exactly one key.
    ///
    /// Both halves of that contract are load-bearing for the *negative* link
    /// dependency ([`Entry::links`]). A probe at a page that is not written yet
    /// is stored through this function, and the page that later appears there is
    /// stored through it too; if the two spell differently the lookup finds
    /// nothing, reads the recorded `None` back as "still nothing here", and the
    /// linking page stays a cache hit serving a link that is still broken. So
    /// the canonicalization has to survive a missing file ([`crate::fs::resolved`],
    /// not [`crate::fs::canonical`], which would keep a symlinked ancestor
    /// unresolved), and a relative path is absolutized against the root rather
    /// than stored as the caller happened to spell it.
    ///
    /// Takes `root` rather than `&self` so [`Cache::load`] can normalize the
    /// link map while building the very value that would own it, without a
    /// second spelling of the rule.
    fn portable(root: &Path, path: &Path) -> PathBuf {
        let resolved = crate::fs::resolved(path);
        let absolute = if resolved.is_absolute() {
            resolved
        } else {
            root.join(resolved)
        };
        absolute
            .strip_prefix(root)
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
    use super::{Cache, GENERATED, SiteInputs};
    use crate::config::Config;
    use crate::content::{Data, Frontmatter, Page, PageId, Siblings};
    use crate::graph::Hash;
    use crate::render::RenderMaps;
    use crate::render::{AssetMap, LinkMap, SrcSets};
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
        let inputs = SiteInputs {
            modules: Hash::of_bytes(b""),
            fonts: None,
        };
        Cache::load(
            &config,
            &inputs,
            Vec::new(),
            RenderMaps {
                links: &LinkMap::default(),
                srcsets: &SrcSets::default(),
                assets: &AssetMap::default(),
            },
            root,
            &Ui::new(Level::Silent),
        )
        .expect("cache")
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
        let listing = cache.key(&page(
            path,
            Data::Generated {
                dict: String::new(),
                lists: Vec::new(),
            },
        ));

        assert_ne!(real, listing);
        assert!(listing.starts_with(GENERATED), "{listing:?}");
    }

    /// The key spelling is a contract, not an accident of where the build ran:
    /// root-relative under the root, absolute outside it, and the same for a
    /// path before and after a file appears there.
    ///
    /// That last part is what the negative link dependency rides on. A probe
    /// recorded at a page that does not exist yet must key exactly like the page
    /// that later shows up there, or the recorded `None` keeps matching "nothing
    /// sits here" and the linking page stays cached with a broken link. A
    /// symlinked content subtree is what separates the two: a file that exists
    /// canonicalizes to its real location, one that does not has nothing to
    /// canonicalize.
    #[test]
    #[cfg(unix)]
    fn portable_keys_do_not_depend_on_whether_the_file_exists() {
        let tmp = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let cache = cache(tmp.path());
        let root = crate::fs::canonical(tmp.path());

        // Under the root: stored relative, and resolves back to the same file.
        std::fs::write(root.join("layout.typ"), "").unwrap();
        let inside = Cache::portable(&root, &root.join("layout.typ"));
        assert_eq!(inside, Path::new("layout.typ"));
        assert_eq!(cache.resolve(&inside), root.join("layout.typ"));

        // Outside the root (the machine-global typst package cache is the real
        // case): stored absolute, and resolves back to itself.
        let external = crate::fs::canonical(outside.path()).join("theme.typ");
        std::fs::write(&external, "").unwrap();
        let key = Cache::portable(&root, &external);
        assert_eq!(key, external);
        assert_eq!(cache.resolve(&key), external);

        // Not on disk yet: the spelling it will get once written, arrived at
        // through the symlink the walker sees rather than the real location.
        std::fs::create_dir_all(root.join("vault/posts")).unwrap();
        std::fs::create_dir_all(root.join("content")).unwrap();
        std::os::unix::fs::symlink(root.join("vault/posts"), root.join("content/posts")).unwrap();
        let missing = root.join("content/posts/b.typ");
        let before = Cache::portable(&root, &missing);
        std::fs::write(root.join("vault/posts/b.typ"), "").unwrap();
        assert_eq!(
            before,
            Cache::portable(&root, &missing),
            "a path must key the same before and after the file appears"
        );
        assert_eq!(before, Path::new("vault/posts/b.typ"));
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
