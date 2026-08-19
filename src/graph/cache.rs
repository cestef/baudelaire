//! Authoritative build cache: content hashes, dependency edges, and rendered
//! output, persisted between builds to drive incremental rebuilds.
//!
//! Layout under the cache directory:
//!
//! ```text
//! manifest.json          # small: config fingerprint + per-page metadata
//! objects/ab/abcd..       # rendered HTML, content-addressed by blob hash
//! ```

use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tracing::debug;

use crate::codegen::Value;
use crate::config::Config;
use crate::content::{Data, Page};
use crate::error::warning::ManifestUnreadable;
use crate::error::{Artifact, Result, SerializeError};
use crate::graph::access::{Root, Roots};
use crate::graph::objects::Objects;
use crate::graph::{Deps, FileDigests, Hash, Portable, Reads, Renderer};
use crate::render::{
    AssetDeps, Finding, Fragments, ImageRef, Inline, LinkDeps, Outbound, RenderMaps, SrcSetDeps,
    Syndicated, Target, UrlDeps, Weight,
};
use crate::ui::Ui;

const MANIFEST: &str = "manifest.json";

/// Manifest key prefix reserved for generated listings, which have no real
/// source file; not a valid relative path under the project root, so it cannot
/// collide with a real page's key.
const GENERATED: &str = "<generated>";

/// A page's cached compile result and the fingerprints that validate it.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Entry {
    hash: Hash,
    /// Dependency files and their hashes at compile time; `None` records one
    /// that could not be hashed, so its later appearance still invalidates.
    deps: BTreeMap<PathBuf, Option<Hash>>,
    /// Injected values the page read (`sys.inputs.baudelaire.git.hash`, ..) and
    /// their digests at compile time; `None` records a read of an absent value,
    /// so its later appearance still invalidates.
    #[serde(default)]
    meta: BTreeMap<String, Option<Hash>>,
    /// The permalinks this page's links resolved against, keyed by the target's
    /// source path; `None` records a link that resolved to no page, so a page
    /// later appearing at that path still invalidates.
    #[serde(default)]
    links: LinkDeps,
    /// The URLs this page's already-URL links named, and whether a page sat
    /// there; `false` records a URL nothing served, so a page appearing there
    /// still invalidates the linker.
    #[serde(default)]
    urls: UrlDeps,
    /// The responsive variants this page's images matched, keyed by the source
    /// path each `<img>` named; `None` records a source with no variants, so an
    /// image that gains some later still invalidates.
    #[serde(default)]
    srcsets: SrcSetDeps,
    /// The processed-asset URLs this page's references resolved to, keyed by
    /// the request path each named; `None` records a reference to an absent
    /// asset, so one appearing later still invalidates.
    #[serde(default)]
    assets: AssetDeps,
    /// Content hash of the rendered HTML; locates its blob in the object store.
    blob: Hash,
    /// What the render pass produced besides the markup, replayed on a hit.
    #[serde(default)]
    outputs: Outputs,
}

/// The render-side results of compiling a page, stored alongside its markup
/// because nothing here can be recovered from the markup afterwards.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Outputs {
    /// Images the page externalized out of the DOM, re-copied into `dist` on a
    /// cache hit.
    pub images: Vec<ImageRef>,
    /// The build-provided assets the page asked for, written again on a cache
    /// hit.
    #[serde(default, skip_serializing_if = "std::collections::BTreeSet::is_empty")]
    pub owned: std::collections::BTreeSet<String>,
    /// Raw targets of the broken internal links the page produced, so the link
    /// check sees a cached page too.
    pub broken: Vec<String>,
    /// The heading ids this page exposes, so the deep-link check sees a cached
    /// page too.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub anchors: Vec<String>,
    /// The links this page carries into a section of another page.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub deep: Vec<Target>,
    /// The pages this page's own content links to.
    #[serde(default, skip_serializing_if = "Outbound::is_empty")]
    pub outbound: Outbound,
    /// The digest of the backlinks this page was compiled with, `None` when it
    /// was compiled with none at all (`links { backlinks }` off).
    ///
    /// Not folded into the cache fingerprint: a page is compiled against a
    /// predicted value, and the repair pass checks the real one against this.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backlinks: Option<Hash>,
    /// The page's head and body markup, captured only while the single-file
    /// export is on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fragments: Option<Fragments>,
    /// The page's prose as a full-content feed publishes it, captured only
    /// while `generate { feed { content "full" } }` is on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub syndicated: Option<Syndicated>,
    /// The `http(s)` links the page carries, stored so `check --external`
    /// probes a cached page's links too.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub external: Vec<String>,
    /// What the lint pass found on the page, stored so the check sees a cached
    /// page too.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub lints: Vec<Finding>,
    #[serde(default, skip_serializing_if = "Weight::is_empty")]
    pub weight: Weight,
    /// The digests of the page's inline scripts and styles, for the generated
    /// content security policy.
    #[serde(default, skip_serializing_if = "Inline::is_empty")]
    pub inline: Inline,
}

/// The serialized cache manifest: metadata only, no page markup.
#[derive(Debug, Default, Serialize, Deserialize)]
struct Manifest {
    /// Fingerprint of the site-wide inputs that produced these entries (config,
    /// asset map, embedded assets); any change invalidates the whole manifest.
    config: Option<Hash>,
    /// Entries keyed by page source path.
    pages: BTreeMap<PathBuf, Entry>,
    /// Entries for the artifacts compiled from many pages (a bundled PDF),
    /// keyed by the artifact's id.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    bundles: BTreeMap<String, Compiled>,
}

/// What validating any compiled artifact needs: the text typst compiled, and
/// every file that compile read.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Compiled {
    /// Hash of the exact text typst compiled.
    hash: Hash,
    /// Dependency files and their hashes at compile time; `None` for one that
    /// could not be hashed.
    deps: BTreeMap<PathBuf, Option<Hash>>,
}

/// What a build reads that no page records reading, and so is fingerprinted
/// whole.
#[derive(std::hash::Hash)]
pub struct SiteInputs {
    /// A content hash of the generated `@baudelaire/*` Typst modules, which
    /// exist only in memory and so resolve to no path the dependency tracker
    /// could see.
    pub modules: Hash,
    /// A content hash of the fonts the site ships, or `None` when it ships
    /// none.
    ///
    /// A face is resolved by name out of a store built by walking a directory,
    /// so no font file is ever read through the world.
    pub fonts: Option<Hash>,
}

/// One freshly compiled page as [`Cache::record`] takes it.
#[derive(Clone, Copy)]
pub struct Recorded<'a> {
    pub page: &'a Page,
    /// Hash of the exact text typst compiled.
    pub fingerprint: Hash,
    pub html: &'a str,
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
    pub outputs: &'a Outputs,
}

/// The previous manifest, and the next one accumulated as pages are reused or
/// recompiled.
pub struct Cache {
    dir: PathBuf,
    root: PathBuf,
    enabled: bool,
    config: Hash,
    prev: Manifest,
    next: Manifest,
    /// Per-build file-hash memo, so a dependency shared by many pages is hashed
    /// once.
    digests: FileDigests,
    /// The tracked injected values, for resolving the digest of a page's
    /// recorded value reads.
    roots: Vec<(String, Value)>,
    /// This build's page-to-permalink map, keyed the way the manifest stores
    /// paths, to revalidate each page's recorded [`Entry::links`].
    links: BTreeMap<PathBuf, String>,
    /// This build's variant digests, to revalidate [`Entry::srcsets`].
    srcsets: BTreeMap<String, Hash>,
    /// This build's request-to-served asset URLs, to revalidate
    /// [`Entry::assets`].
    assets: BTreeMap<String, String>,
    /// Every URL this build serves a page at, to revalidate [`Entry::urls`].
    urls: HashSet<String>,
    objects: Objects,
}

impl Cache {
    /// Load the cache for a build.
    ///
    /// With incremental builds off it still records the next manifest, and
    /// fingerprints it identically to a normal build's, so `--no-cache` costs
    /// one cold build rather than poisoning the next.
    pub fn load(
        config: &Config,
        inputs: &SiteInputs,
        roots: Vec<(String, Value)>,
        maps: RenderMaps<'_>,
        root: &Path,
        dir: PathBuf,
        ui: &Ui,
    ) -> Result<Self> {
        let manifest = dir.join(MANIFEST);
        let prev = fs::read(&manifest).map_or_else(
            |_| Manifest::default(),
            |bytes| match serde_json::from_slice(&bytes) {
                Ok(prev) => prev,
                Err(e) => {
                    ui.warn(ManifestUnreadable {
                        path: manifest,
                        source: e,
                    });
                    Manifest::default()
                }
            },
        );
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
                .map(|(source, permalink)| (Portable(&root).key(source), permalink.to_owned()))
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
    /// Paths recorded as absolute are left out: those lie outside the project
    /// (typst's package cache), and nothing there is edited by hand.
    pub fn read(&self) -> impl Iterator<Item = PathBuf> + '_ {
        self.next
            .pages
            .values()
            .flat_map(|entry| entry.deps.keys())
            .filter(|dep| dep.is_relative())
            .map(|dep| self.root.join(dep))
    }

    fn roots(&self) -> Roots<'_> {
        self.roots.iter().map(Root::from).collect()
    }

    /// The links the last build recorded for `page`, if it has seen it.
    ///
    /// What this build's backlinks are predicted from, never trusted: what a
    /// page is compiled with is checked against the graph this build produces,
    /// and the pages that disagree are compiled again.
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
        match self.hit(page, fingerprint) {
            Ok(reused) => Some(reused),
            Err(miss) => {
                debug!(page = %page.source.display(), why = miss.why(), "cache miss");
                None
            }
        }
    }

    /// The cached page, or what stopped it being one.
    fn hit(&mut self, page: &Page, fingerprint: &Hash) -> Result<(String, Outputs), Miss> {
        if !self.enabled {
            return Err(Miss::Disabled);
        }
        if self.prev.config.as_ref() != Some(&self.config) {
            return Err(Miss::Config);
        }
        let key = self.key(page);
        let entry = self.prev.pages.get(&key).ok_or(Miss::Unrecorded)?;
        if &entry.hash != fingerprint {
            return Err(Miss::Source);
        }
        if !self.intact(&entry.deps) {
            return Err(Miss::Deps);
        }
        let roots = self.roots();
        if !entry
            .meta
            .iter()
            .all(|(key, hash)| roots.digest(key) == *hash)
        {
            return Err(Miss::Roots);
        }
        if !entry
            .links
            .iter()
            .all(|(path, permalink)| self.links.get(path) == permalink.as_ref())
        {
            return Err(Miss::Links);
        }
        if !entry
            .urls
            .iter()
            .all(|(url, served)| self.urls.contains(url) == *served)
        {
            return Err(Miss::Urls);
        }
        if !entry
            .srcsets
            .iter()
            .all(|(source, digest)| self.srcsets.get(source) == digest.as_ref())
        {
            return Err(Miss::Srcsets);
        }
        if !entry
            .assets
            .iter()
            .all(|(request, served)| self.assets.get(request) == served.as_ref())
        {
            return Err(Miss::Assets);
        }
        let entry = entry.clone();
        let html = self.objects.read(&entry.blob).ok_or(Miss::Blob)?;
        let outputs = entry.outputs.clone();
        self.next.pages.insert(key, entry);
        Ok((html, outputs))
    }

    /// Whether every file a compile read still hashes to what it hashed then.
    fn intact(&self, deps: &BTreeMap<PathBuf, Option<Hash>>) -> bool {
        deps.iter()
            .all(|(path, hash)| self.digests.of(&Portable(&self.root).resolve(path)) == *hash)
    }

    /// The files a compile read, hashed and keyed the portable way the manifest
    /// stores them.
    fn digested(&self, deps: &Deps) -> BTreeMap<PathBuf, Option<Hash>> {
        deps.files()
            .iter()
            .map(|p| (Portable(&self.root).key(p), self.digests.of(p)))
            .collect()
    }

    /// Whether a bundled document compiled from `fingerprint` can be left as it
    /// is: the same module text, every file it read unchanged, and the file
    /// still on disk.
    ///
    /// The disk check is load-bearing: a bundle is written only by the build
    /// that compiles it, so a `dist` cleared behind the cache's back would
    /// leave it missing forever.
    pub fn reuse_bundle(&mut self, id: &str, fingerprint: &Hash, path: &Path) -> bool {
        let hit = self.enabled
            && self.prev.config.as_ref() == Some(&self.config)
            && path.exists()
            && self
                .prev
                .bundles
                .get(id)
                .is_some_and(|entry| &entry.hash == fingerprint && self.intact(&entry.deps));
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
            .map(|(path, permalink)| (Portable(&self.root).key(path), permalink.clone()))
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
    /// A repair draws no sidecars, so the compile-side records (`deps`, `meta`)
    /// are unioned with the first compile's, or a card template's files and
    /// value reads go unchecked. The render-side records (`links`, `urls`,
    /// `srcsets`, `assets`) must not be unioned: a repair re-renders the whole
    /// page, so what it saw is the complete set.
    pub fn relink(&mut self, compiled: Recorded<'_>) {
        let key = self.key(compiled.page);
        let inherited = self
            .next
            .pages
            .get(&key)
            .map(|entry| (entry.deps.clone(), entry.meta.clone()))
            .unwrap_or_default();
        self.record(compiled);
        if let Some(entry) = self.next.pages.get_mut(&key) {
            let (deps, meta) = inherited;
            let fresh = std::mem::replace(&mut entry.deps, deps);
            entry.deps.extend(fresh);
            let fresh = std::mem::replace(&mut entry.meta, meta);
            entry.meta.extend(fresh);
        }
    }

    /// Persist the manifest and every referenced HTML blob, then drop objects
    /// no longer referenced.
    ///
    /// `outputs` supplies the HTML for freshly recorded pages; a cache hit's
    /// blob is already stored under the same address.
    pub fn save<'a>(&self, outputs: impl IntoIterator<Item = (&'a Page, &'a str)>) -> Result<()> {
        crate::fs::create_dir_all(&self.dir)?;
        let html: BTreeMap<PathBuf, &str> = outputs
            .into_iter()
            .map(|(page, html)| (self.key(page), html))
            .collect();
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

    /// The manifest key for a page: portable (see [`Portable::key`]), with
    /// generated listings under a reserved prefix, since their fabricated
    /// source path could otherwise collide with a real page's.
    fn key(&self, page: &Page) -> PathBuf {
        let path = Portable(&self.root).key(&page.source);
        match page.data {
            Data::Generated { .. } => Path::new(GENERATED).join(path),
            #[cfg(feature = "markdown")]
            Data::Lowered { .. } => path,
            Data::Export | Data::Empty => path,
        }
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
            config.cache.dir.clone(),
            &Ui::new(Level::Silent),
        )
        .expect("cache")
    }

    /// A generated listing fabricates a source path that never exists on disk.
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

    /// A warm cache still matches after the site moves on disk.
    #[test]
    fn keys_are_relative_to_the_project_root() {
        let tmp = tempfile::tempdir().unwrap();
        let cache = cache(tmp.path());
        let path = tmp.path().join("content/posts/a.typ");

        let key = cache.key(&page(path.to_str().expect("utf-8 tempdir"), Data::Empty));

        assert_eq!(key, Path::new("content/posts/a.typ"));
    }
}

/// Why a page could not be served from the cache: the answer to "why did this
/// rebuild?", which is the one thing an incremental build is asked most often.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Miss {
    /// `--no-cache`, or `cache { incremental #false }`.
    Disabled,
    /// The config, the generated modules or the fonts changed, which nothing
    /// per-page can vouch for.
    Config,
    /// No manifest entry: a new page, or a cache that never saw it.
    Unrecorded,
    /// The page's own source, or the wrapper binding it to its template.
    Source,
    /// A file the compile read.
    Deps,
    /// A value the page took off the site's data roots.
    Roots,
    /// A permalink one of its links resolved to.
    Links,
    /// Whether a URL it named is served by this site.
    Urls,
    /// The width variants one of its images matched.
    Srcsets,
    /// An asset it referenced.
    Assets,
    /// The recorded HTML is gone from the object store.
    Blob,
}

impl Miss {
    /// One word for the log, so a run can be grepped by reason.
    const fn why(self) -> &'static str {
        match self {
            Self::Disabled => "cache disabled",
            Self::Config => "site inputs changed",
            Self::Unrecorded => "not in the manifest",
            Self::Source => "source changed",
            Self::Deps => "a file it reads changed",
            Self::Roots => "site data changed",
            Self::Links => "a link target moved",
            Self::Urls => "a url it names appeared or went",
            Self::Srcsets => "an image variant changed",
            Self::Assets => "an asset it references changed",
            Self::Blob => "its recorded html is gone",
        }
    }
}
