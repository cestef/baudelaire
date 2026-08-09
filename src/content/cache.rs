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

/// What a page's `source` resolved to.
///
/// One variant per body dialect, and the reader follows the *file*: a page names
/// a declared file and gets whatever that file is, rather than whatever the page
/// itself is written in. `DiscoveryCache::READERS` is the extension each answers
/// to.
/// Turns a declared file into a body: its name, the path as declared, the path
/// as resolved, and its text.
#[cfg(feature = "markdown")]
type Reader = fn(&str, &Path, &Path, String) -> Sourced;

#[cfg(feature = "markdown")]
enum Sourced {
    /// Markdown, read here and lowered under its own name, so a fault in it is
    /// reported where the prose is rather than against the stub that named it.
    Markdown { named: String, text: String },
    /// Typst, which the compiler opens itself through the mount
    /// [`crate::world::module::Sources`] installs. The page's body is the one
    /// `include` that names it, so the file's own spans, its dependency on the
    /// page, and everything else that follows from typst opening a file are had
    /// for free. Nothing of it is read here but the reading estimate.
    Typst {
        include: String,
        reading: crate::engine::text::Reading,
    },
}

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
        let salt = Self::salt(config, project.modules());
        let dir = config.cache.dir.clone();
        let prev = std::fs::read(dir.join(MANIFEST))
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Manifest>(&bytes).ok())
            // a manifest built under different config, or against different
            // generated modules, can't be trusted.
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
        if path.extension().is_some_and(|e| e == Config::MARKDOWN) {
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

        // A `source` moves the body to another file, so everything below reads
        // that file's text under that file's name: a fault the lowering finds is
        // reported where the prose is, not against the stub that named it.
        let sourced = Self::sourced(&frontmatter, &document, path, config, &origin)?;
        // A typst source is not lowered at all: it is a file the compiler opens,
        // and the page's body is the one line that names it.
        if let Some(Sourced::Typst { include, reading }) = &sourced {
            let sourcemap = crate::content::SourceMap::new(text.clone(), include.len(), Vec::new());
            let data = Data::Lowered {
                dict,
                sourcemap: std::sync::Arc::new(sourcemap),
                reading: *reading,
            };
            return Ok((frontmatter, data, include.clone()));
        }
        let (document, text, named) = match &sourced {
            Some(Sourced::Markdown { named, text }) => {
                (Document::whole(text), text.as_str(), named.clone())
            }
            _ => (document, text.as_str(), named),
        };

        // Measured here, on the body the author wrote: what the lowering
        // produces is Typst code line for line, and a reading estimate taken
        // from *that* counts none of the prose. See [`Data::Lowered`].
        let reading = crate::engine::text::Reading::markdown(document.body);
        let data = |sourcemap| Data::Lowered {
            dict,
            sourcemap,
            reading,
        };
        let (body, sourcemap) =
            Markdown::new(&document, text, &named, &config.content.markdown).lower()?;
        Ok((frontmatter, data(std::sync::Arc::new(sourcemap)), body))
    }

    /// The dialects a declared source can be a body in: the extension that
    /// names each, beside the reader that turns the file into a body. Adding one
    /// is a row here and a [`Sourced`] variant, and nothing else: the row *is*
    /// the dispatch, so a dialect cannot be listed and left unhandled, and the
    /// error's help lists exactly the readers the build has.
    ///
    /// Gated with its one caller: `source` replaces a *markdown* page's body,
    /// and a binary without that feature has no such page to give one to.
    #[cfg(feature = "markdown")]
    const READERS: &'static [(&'static str, Reader)] = &[
        // Markdown: lowered here, under its own name, so a fault in it is
        // reported where the prose is rather than against the stub that named
        // it.
        (Config::MARKDOWN, |_, _, file, text| Sourced::Markdown {
            named: file.display().to_string(),
            text,
        }),
        // Typst: the compiler opens it through the mount, so the body is the one
        // `include` that names it and every span inside the file is typst's own.
        // The text is read for the reading estimate and nothing else.
        (Config::TYPST, |name, declared, _, text| Sourced::Typst {
            include: format!(
                "#include {}",
                crate::codegen::Typst(&crate::codegen::Value::str(
                    crate::world::module::Sources::vpath(name, declared)
                ))
            ),
            reading: crate::engine::text::Reading::of(&text),
        }),
    ];

    /// The extensions [`READERS`](Self::READERS) claims, for the error that
    /// reports one it does not.
    #[cfg(feature = "markdown")]
    fn readable() -> Vec<&'static str> {
        Self::READERS.iter().map(|(named, _)| *named).collect()
    }

    /// The file a page's `source` names, read: its display name and its text.
    ///
    /// The name is resolved against `paths { sources { } }` and nowhere else, so
    /// a page can only ever reach a file the config already offered it. An
    /// undeclared name is an error rather than a path to try, which is the
    /// difference between a key that selects and a key that opens.
    ///
    /// The declared path is joined to the project root, and is allowed to leave
    /// it: that is the config's call to make, and the reason the declaration
    /// lives in a section a theme may not write.
    ///
    /// Gated with its one caller: `source` replaces a *markdown* body, and
    /// without that feature there is no markdown page to give one to. A `.typ`
    /// page carrying the key is refused either way, in `Page::load`.
    #[cfg(feature = "markdown")]
    fn sourced(
        frontmatter: &Frontmatter,
        document: &crate::content::markdown::Document<'_>,
        path: &Path,
        config: &Config,
        origin: &Origin<'_>,
    ) -> Result<Option<Sourced>> {
        let Some(name) = &frontmatter.source else {
            return Ok(None);
        };
        // Every refusal below is about the key the author wrote, so each is
        // raised at it: the page is named either way, but a page with a dozen
        // frontmatter lines does not say which one this is about.
        let (text, at) = (origin.text(), origin.entry(Frontmatter::SOURCE));
        if !document.body.trim().is_empty() {
            return Err(crate::error::ContentError::source_and_body(path, text, at).into());
        }
        let declared = config.paths.source(name).ok_or_else(|| {
            crate::error::ContentError::unknown_source(
                path,
                name,
                &config.paths.declared(),
                text,
                at,
            )
        })?;
        // The reader follows the *file*, not the page that names it: the row
        // that claims the extension is the row that reads it, so a dialect
        // cannot be listed and then fall through to somebody else's reader. It
        // did, and a file of another kind came out as prose with its own syntax
        // in it, on a green build.
        let ext = declared.extension().and_then(|e| e.to_str()).unwrap_or("");
        let read = Self::READERS
            .iter()
            .find(|(named, _)| *named == ext)
            .map(|(_, read)| read)
            .ok_or_else(|| {
                crate::error::ContentError::source_unreadable(
                    name,
                    declared,
                    &Self::readable(),
                    text,
                    at,
                )
            })?;
        let file = config.root.join(declared);
        let text = Self::decode(&crate::fs::read(&file)?)
            .ok_or_else(|| crate::error::ContentError::non_utf8_source(&file))?;
        Ok(Some(read(name, declared, &file, text)))
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
    ///
    /// The last two terms are both "what a frontmatter *evaluation* can read
    /// that no per-page probe will ever see it read", which is the same
    /// question [`SiteInputs`](crate::graph::SiteInputs) answers for the
    /// compile cache. A frontmatter is produced by evaluating the page's typst
    /// module, so anything that evaluation reaches is an input to the cached
    /// value:
    ///
    /// - the generated `@baudelaire/*` modules, by content. One is served from
    ///   memory and resolves to no path, so it can never appear in an
    ///   [`Entry`]'s dependencies. Without this,
    ///   `#import "@baudelaire/site": title` in a frontmatter froze at whatever
    ///   the site was called when the page was first cached: the body
    ///   re-evaluated and the frontmatter did not, so one page emitted two
    ///   titles, and a stale `slug` published at a URL the rest of the build no
    ///   longer agreed on.
    ///
    /// - `paths`, for the declared-source mount. A source's virtual path is
    ///   `/<prefix>/<name>.<ext>`, so re-pointing a name at another file of the
    ///   same kind leaves both the module *and* every page's source byte for
    ///   byte identical. The real file is what lands in `deps`, and the old one
    ///   is still there and still unchanged, so every entry reads as valid
    ///   while every frontmatter is derived from a file the config no longer
    ///   names. Taken whole rather than as `paths.sources` alone: this is the
    ///   second hole of exactly this shape found in this salt, and a `paths`
    ///   edit is a rare enough thing to re-derive frontmatter over.
    fn salt(config: &Config, modules: Hash) -> Hash {
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
        Hash::of(&(keys, schemas, &config.paths, modules, Renderer::current()))
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
            DiscoveryCache::salt(
                &crate::config::Config::parse(text).expect("should parse"),
                crate::graph::Hash::of_bytes(b""),
            )
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

    /// A frontmatter is produced by evaluating the page's typst module, which
    /// may import a generated `@baudelaire/*` one. Those are served from memory
    /// and resolve to no path, so no page can ever record one as a dependency
    /// and only the salt can notice one changed.
    #[test]
    fn the_salt_covers_the_generated_modules() {
        let config = crate::config::Config::parse("site \"s\"").expect("should parse");
        assert_ne!(
            DiscoveryCache::salt(&config, crate::graph::Hash::of_bytes(b"one")),
            DiscoveryCache::salt(&config, crate::graph::Hash::of_bytes(b"two"))
        );
    }

    /// Re-pointing a declared source at another file of the same kind changes
    /// neither the generated module (the mount path carries the name and the
    /// extension, not the target) nor any page's bytes, and leaves the old
    /// file on disk unchanged. Only the salt is left to notice.
    #[test]
    fn the_salt_covers_which_file_a_declared_source_names() {
        let salt = |path: &str| {
            let text = format!("paths {{ sources {{ meta \"{path}\" }} }}");
            DiscoveryCache::salt(
                &crate::config::Config::parse(&text).expect("should parse"),
                crate::graph::Hash::of_bytes(b""),
            )
        };
        assert_ne!(salt("data/a.json"), salt("data/b.json"));
    }
}
