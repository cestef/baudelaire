//! Discovery cache: persisted, extracted frontmatter.
//!
//! Building the page set means reading every page's `#let frontmatter`, which
//! requires *evaluating* the page's typst module, the build's dominant cost on
//! an otherwise-unchanged rebuild, since the compiled output is already cached
//! but the frontmatter is re-derived from scratch each time.
//!
//! This cache stores each page's extracted [`Frontmatter`] against a fingerprint
//! of its source and every file the evaluation read. An unchanged page reuses
//! the stored frontmatter and skips the evaluation entirely; a changed page (or
//! a changed dependency, or a taxonomy-config change) re-evaluates and restores.
//! Correctness mirrors the compile cache: the same dependency tracking, so an
//! edit to an imported file the frontmatter reads invalidates it.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use typst::foundations::Module;

use crate::codegen::Value;
use crate::config::Config;
use crate::content::page::Data;
use crate::content::{Frontmatter, Origin};
use crate::error::{Artifact, Result, SerializeError};
use crate::graph::{Analyzer, FileDigests, Hash, Renderer, Root, Roots};
use crate::world::Project;

/// The on-disk discovery manifest, beside the compile cache's `manifest.json`.
const MANIFEST: &str = "discovery.json";

/// One page's cached frontmatter and the fingerprints that validate it.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Entry {
    /// Hash of the page's own source text.
    source: Hash,
    /// Files the frontmatter evaluation read (transitive imports, data loaders),
    /// with their hashes at evaluation time. A change to any re-evaluates.
    ///
    /// `None` records a file that was not there to hash, so its later
    /// appearance re-evaluates too, exactly as the compile cache's `deps` does.
    /// An unhashable read used to be dropped from the entry entirely, and the
    /// generated typst tables are read *during* discovery, before the build has
    /// written them: on a cold build a page whose frontmatter reads
    /// `@baudelaire/pages` recorded no dependency on it at all, and every later
    /// build carried that entry forward. The count it printed was the empty
    /// table's, for ever.
    deps: BTreeMap<PathBuf, Option<Hash>>,
    /// The injected values the evaluation read (`sys.inputs.baudelaire.git.hash`,
    /// the build clock), and their digests then. `None` records a read of an
    /// absent value, so its later appearance re-evaluates too.
    ///
    /// A file dependency cannot stand in for these: they go through the `World`
    /// and leave no path behind. Without them, frontmatter derived from build
    /// metadata (`title: "Docs @ " + git.hash`, a date from `datetime.today()`)
    /// was extracted once and frozen for ever, while the *compile* dutifully
    /// re-ran and re-emitted the stale value it was handed.
    #[serde(default)]
    meta: BTreeMap<String, Option<Hash>>,
    /// The extracted frontmatter.
    frontmatter: Frontmatter,
    /// Whether the module exported a `frontmatter` binding (vs. defaulted).
    export: bool,
}

/// The serialized discovery manifest.
#[derive(Debug, Default, Serialize, Deserialize)]
struct Manifest {
    /// Fingerprint of the config inputs that change how frontmatter is
    /// interpreted or judged. A change invalidates every entry: a key that was
    /// `extra` yesterday may be a taxonomy today, and frontmatter that passed
    /// its collection's schema yesterday may not satisfy today's.
    salt: Option<Hash>,
    /// Entries keyed by page source path.
    pages: BTreeMap<PathBuf, Entry>,
}

/// Persisted extracted frontmatter, so discovery skips the typst module
/// evaluation for pages whose source and dependencies are unchanged.
pub struct DiscoveryCache<'a> {
    dir: PathBuf,
    enabled: bool,
    salt: Hash,
    prev: Manifest,
    /// The tracked value trees and a per-file memo, for resolving which injected
    /// values a page's frontmatter read. The same analysis the compile cache
    /// runs, over the same roots.
    analyzer: Analyzer<'a>,
    /// The manifest being accumulated this build. Filled during the parallel
    /// page load, hence the lock; contention is negligible (a map insert).
    next: Mutex<Manifest>,
    /// Per-build file-hash memo: a module imported by many pages is hashed once
    /// while validating their dependencies, not once per page.
    digests: FileDigests,
}

impl<'a> DiscoveryCache<'a> {
    /// Load the cache for a build. When incremental builds are disabled it never
    /// reports a hit and never persists: every page evaluates live.
    /// `tracked` is the build's injected value trees, owned by the caller
    /// because [`Roots`] borrows them, exactly as the compile side's
    /// [`Pass`](crate::engine) holds them for its own analyzer.
    pub fn load(config: &Config, project: &'a Project, tracked: &'a [(String, Value)]) -> Self {
        let salt = Self::salt(config);
        let dir = config.cache.dir.clone();
        let prev = std::fs::read(dir.join(MANIFEST))
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Manifest>(&bytes).ok())
            // a manifest built under different taxonomy config can't be trusted.
            .filter(|m| m.salt.as_ref() == Some(&salt))
            .unwrap_or_default();
        Self {
            dir,
            enabled: config.cache.incremental,
            salt,
            prev,
            next: Mutex::new(Manifest::default()),
            digests: FileDigests::default(),
            analyzer: Analyzer::new(tracked.iter().map(Root::from).collect::<Roots>(), project),
        }
    }

    /// Load a page's extracted frontmatter, whether the module exported one, and
    /// its body text, reusing the cached frontmatter when the source and every
    /// dependency are unchanged.
    ///
    /// On a hit the page's typst module is never touched: neither parsed nor
    /// evaluated. The body is decoded straight from the file bytes (which equal
    /// `Source::text`; typst stores the text verbatim, stripping only a leading
    /// UTF-8 BOM), and the legacy-syntax check is skipped because the cached
    /// entry could only have been written after a build that already passed it
    /// on identical content. On a miss the module is parsed, checked, evaluated,
    /// and the result recorded for the next build.
    pub fn load_page(
        &self,
        collection: &str,
        path: &Path,
        config: &Config,
        project: &Project,
    ) -> Result<(Frontmatter, Data, String)> {
        // A markdown page has no typst module to evaluate, so none of the
        // machinery below applies: it is read, split, and lowered. It skips this
        // cache deliberately rather than for want of wiring -- what the cache
        // buys is skipping a typst *evaluation*, and there is not one. Parsing
        // markdown is microseconds, and the compile cache still covers the page
        // through its wrapper fingerprint.
        #[cfg(feature = "markdown")]
        if path.extension().is_some_and(|e| e == "md") {
            return Self::load_markdown(collection, path, config);
        }
        // Fast path: unchanged source and dependencies reuse the stored
        // frontmatter with no typst parse or evaluation at all.
        if self.enabled
            && let Some(body) = Self::decode(&crate::fs::read(path)?)
        {
            let hash = Hash::of_bytes(body.as_bytes());
            if let Some(entry) = self.reuse(path, hash) {
                return Ok((entry.frontmatter, Data::of(entry.export), body));
            }
        }
        // Miss (or caching disabled, or non-UTF-8): parse and evaluate the page.
        let source = project.source(path)?;
        Frontmatter::check(&source, path)?;
        let origin = Origin::new(&source, path, collection);
        let hash = Hash::of_bytes(source.text().as_bytes());
        let (frontmatter, export) = if self.enabled {
            let (module, deps, clock) = project.module_tracked(&source)?;
            let extracted = Self::interpret(&module, &origin, config)?;
            // Which injected values the evaluation read, across the page's own
            // source and every `.typ` it imported. The clock goes through the
            // `World` and leaves no file behind, so it is recorded under the key
            // it shares with `sys.inputs.baudelaire.date`, as the compile does.
            let mut reads = self.analyzer.reads(&source, &deps);
            if clock {
                reads.insert(Project::clock());
            }
            let meta = self.roots().digests(&reads);
            let deps = deps
                .files()
                .iter()
                .map(|p| (p.clone(), self.digests.of(p)))
                .collect();
            self.next.lock().pages.insert(
                path.to_owned(),
                Entry {
                    source: hash,
                    deps,
                    meta,
                    frontmatter: extracted.0.clone(),
                    export: extracted.1,
                },
            );
            extracted
        } else {
            Self::interpret(&project.module(&source)?, &origin, config)?
        };
        Ok((frontmatter, Data::of(export), source.text().to_owned()))
    }

    /// Read a markdown page: its frontmatter block, in whichever dialect its
    /// fence opened, as the dict every page's template receives, and its body as
    /// the Typst it compiles as.
    #[cfg(feature = "markdown")]
    fn load_markdown(
        collection: &str,
        path: &Path,
        config: &Config,
    ) -> Result<(Frontmatter, Data, String)> {
        use crate::content::markdown::{Document, Markdown};

        let text = Self::decode(&crate::fs::read(path)?)
            .ok_or_else(|| crate::error::ContentError::non_utf8_source(path))?;
        let named = path.display().to_string();
        let document = Document::split(&text, &named)?;

        // Whichever dialect the fence opened, read into the dict and the spans
        // every reader below this line already takes.
        let block = document.block(&named, &text)?;
        let dict = block.dict;
        let origin = Origin::block(&text, &block.spans, path, collection);
        let frontmatter = Frontmatter::from_dict(&dict, &origin, config)?;

        let value = crate::codegen::Value::from(&typst::foundations::Value::Dict(dict));
        let dict = crate::codegen::Typst(&value).to_string();
        let data = |sourcemap| Data::Lowered { dict, sourcemap };
        let (body, sourcemap) =
            Markdown::new(&document, &text, &named, &config.content.markdown).lower()?;
        Ok((frontmatter, data(std::sync::Arc::new(sourcemap)), body))
    }

    /// The previous entry for `path` if it is still valid, its source and every
    /// dependency hash unchanged, carried into the next manifest so it survives
    /// to the following build.
    fn reuse(&self, path: &Path, hash: Hash) -> Option<Entry> {
        let entry = self.prev.pages.get(path)?;
        if entry.source != hash {
            return None;
        }
        if !entry.deps.iter().all(|(p, h)| self.digests.of(p) == *h) {
            return None;
        }
        // ...and every injected value it read must still digest to the same
        // thing, so a new commit or a rolled-over day re-derives the frontmatter
        // that displays it and leaves every other page alone.
        let roots = self.roots();
        if !entry
            .meta
            .iter()
            .all(|(key, hash)| roots.digest(key) == *hash)
        {
            return None;
        }
        self.next
            .lock()
            .pages
            .insert(path.to_owned(), entry.clone());
        Some(entry.clone())
    }

    /// Borrow the tracked roots for a value-digest resolution.
    fn roots(&self) -> Roots<'_> {
        self.analyzer.roots()
    }

    /// Decode file bytes to text exactly as typst does when it builds a
    /// `Source`: a leading UTF-8 BOM is stripped, nothing else is transformed.
    /// `None` for non-UTF-8 input, which routes the caller to the parse path
    /// where typst raises the proper diagnostic.
    fn decode(bytes: &[u8]) -> Option<String> {
        const BOM: &[u8] = b"\xef\xbb\xbf";
        let rest = bytes.strip_prefix(BOM).unwrap_or(bytes);
        std::str::from_utf8(rest).ok().map(str::to_owned)
    }

    /// Read the `frontmatter` export from an evaluated module, defaulting when
    /// the module exports none.
    fn interpret(module: &Module, origin: &Origin, config: &Config) -> Result<(Frontmatter, bool)> {
        match Frontmatter::extract(module, origin, config)? {
            Some(frontmatter) => Ok((frontmatter, true)),
            None => Ok((Frontmatter::default(), false)),
        }
    }

    /// Persist the accumulated manifest. A no-op when disabled.
    pub fn save(&self) -> Result<()> {
        if !self.enabled {
            return Ok(());
        }
        let mut manifest = std::mem::take(&mut *self.next.lock());
        manifest.salt = Some(self.salt);
        let json =
            serde_json::to_vec(&manifest).map_err(|e| SerializeError::new(Artifact::Cache, e))?;
        crate::fs::create_dir_all(&self.dir)?;
        crate::fs::write(self.dir.join(MANIFEST), &json)?;
        Ok(())
    }

    /// Fingerprint the inputs that change frontmatter interpretation: the set of
    /// configured taxonomy keys (which keys are collected as taxonomies rather
    /// than passed through to `extra`), and the renderer that parsed them, so an
    /// upgrade that changes how a frontmatter value is read is not a cache hit.
    ///
    /// The collection schemas join them, with the globs that decide which
    /// collection a page belongs to. A hit reuses a validation as much as an
    /// extraction: without the schemas, tightening one would leave every
    /// unchanged page passing under the old one, and without the globs, moving
    /// a page into a stricter collection by editing only the config would too.
    fn salt(config: &Config) -> Hash {
        let keys: Vec<&str> = config
            .content
            .taxonomies
            .iter()
            .map(|(_, t)| t.key.as_str())
            .collect();
        let schemas: Vec<_> = config
            .content
            .collections
            .iter()
            .map(|(id, c)| (id, &c.glob, &c.schema))
            .collect();
        Hash::of(&(keys, schemas, Renderer::current()))
    }
}

#[cfg(test)]
mod tests {
    use super::DiscoveryCache;

    fn decode(bytes: &[u8]) -> Option<String> {
        DiscoveryCache::decode(bytes)
    }

    #[test]
    fn decode_matches_typst_source_text() {
        // typst strips a leading UTF-8 BOM and transforms nothing else; in
        // particular CRLF line endings are preserved verbatim in `Source::text`,
        // so the parse-free hit path must preserve them too.
        assert_eq!(decode(b"hello").as_deref(), Some("hello"));
        assert_eq!(decode(b"\xef\xbb\xbfhello").as_deref(), Some("hello"));
        assert_eq!(decode(b"a\r\nb\rc\n").as_deref(), Some("a\r\nb\rc\n"));
        // A lone BOM strips to empty; a mid-text BOM is left untouched.
        assert_eq!(decode(b"\xef\xbb\xbf").as_deref(), Some(""));
        assert_eq!(decode(b"a\xef\xbb\xbfb").as_deref(), Some("a\u{feff}b"));
        // Invalid UTF-8 routes to the parse path.
        assert_eq!(decode(b"\xff\xfe"), None);
    }

    /// A hit reuses a page's *validation* as much as its extraction: the page
    /// never re-evaluates, so the schema it was judged against has to be part
    /// of what makes the entry valid. Tightening a schema, or moving a page
    /// into a stricter collection by editing only a glob, would otherwise leave
    /// every unchanged page passing under the old rules.
    #[test]
    fn the_salt_covers_the_schemas_and_the_globs_that_select_their_pages() {
        let salt = |text: &str| {
            DiscoveryCache::salt(&crate::config::Config::parse(text).expect("should parse"))
        };
        let none = salt("content { collections { blog { sort \"date\" } } }");
        let required = salt("content { collections { blog { schema { hero \"str\" } } } }");
        let optional =
            salt("content { collections { blog { schema { hero \"str\" optional=#true } } } }");
        let typed = salt("content { collections { blog { schema { hero \"list\" } } } }");
        let globbed =
            salt("content { collections { blog \"posts/**\" { schema { hero \"str\" } } } }");
        assert_ne!(none, required);
        assert_ne!(required, optional);
        assert_ne!(required, typed);
        assert_ne!(required, globbed);
        // A setting that changes neither which pages are judged nor how is not
        // a reason to re-evaluate every page on the site.
        assert_eq!(
            salt("content { collections { blog { sort \"date\" } } }"),
            salt("content { collections { blog { sort \"title\"; reverse #true } } }")
        );
    }
}
