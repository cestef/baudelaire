//! Discovery cache: each page's extracted [`Frontmatter`], stored against a
//! fingerprint of its source and every file its evaluation read.

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

/// Turns a declared file into a body: its name, the path as declared, the path
/// as resolved, and its text.
#[cfg(feature = "markdown")]
type Reader = fn(&str, &Path, &Path, String) -> Sourced;

/// What a page's `source` resolved to, one variant per body dialect.
#[cfg(feature = "markdown")]
enum Sourced {
    /// Markdown, read here and lowered under its own name, so a fault in it is
    /// reported where the prose is rather than against the stub that named it.
    Markdown { named: String, text: String },
    /// Typst, which the compiler opens itself through the mount
    /// [`crate::world::module::Sources`] installs, so the page's body is the
    /// one `include` that names it.
    Typst {
        include: String,
        reading: crate::engine::text::Reading,
    },
}

/// One page's cached frontmatter and the fingerprints that validate it.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Entry {
    source: Hash,
    /// Files the frontmatter evaluation read, with their hashes then; `None`
    /// records a file that was not there to hash, so its later appearance
    /// re-evaluates too.
    deps: BTreeMap<PathBuf, Option<Hash>>,
    /// The injected values the evaluation read
    /// (`sys.inputs.baudelaire.git.hash`, the build clock) and their digests
    /// then; `None` records a read of an absent value, so its later appearance
    /// re-evaluates too.
    #[serde(default)]
    meta: BTreeMap<String, Option<Hash>>,
    frontmatter: Frontmatter,
    /// Whether the module exported a `frontmatter` binding (vs. defaulted).
    export: bool,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct Manifest {
    /// Fingerprint of the config inputs that change how frontmatter is
    /// interpreted or judged; a change invalidates every entry.
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
    /// The tracked value trees and a per-file memo, for resolving which
    /// injected values a page's frontmatter read.
    analyzer: Analyzer<'a>,
    /// The manifest being accumulated this build, filled during the parallel
    /// page load, hence the lock.
    next: Mutex<Manifest>,
    /// Per-build file-hash memo: a module imported by many pages is hashed
    /// once, not once per page.
    digests: FileDigests,
}

impl<'a> DiscoveryCache<'a> {
    /// Load the cache for a build. When incremental builds are disabled it
    /// never reports a hit and never persists, and `tracked` is the build's
    /// injected value trees, owned by the caller because [`Roots`] borrows
    /// them.
    pub fn load(config: &Config, project: &'a Project, tracked: &'a [(String, Value)]) -> Self {
        let salt = Self::salt(config, project.modules());
        let dir = config.cache.dir.clone();
        let prev = std::fs::read(dir.join(MANIFEST))
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Manifest>(&bytes).ok())
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

    /// Load a page's extracted frontmatter, whether the module exported one,
    /// and its body text, reusing the cached frontmatter when the source and
    /// every dependency are unchanged. On a hit the page's typst module is
    /// neither parsed nor evaluated, and its body is decoded straight from the
    /// bytes.
    pub fn load_page(
        &self,
        collection: &str,
        path: &Path,
        config: &Config,
        project: &Project,
    ) -> Result<(Frontmatter, Data, String)> {
        #[cfg(feature = "markdown")]
        if Config::has_ext(path, Config::MARKDOWN) {
            return Self::load_markdown(collection, path, config);
        }
        if self.enabled
            && let Some(body) = Self::decode(&crate::fs::read(path)?)
        {
            let hash = Hash::of_bytes(body.as_bytes());
            if let Some(entry) = self.reuse(path, hash) {
                return Ok((entry.frontmatter, Data::of(entry.export), body));
            }
        }
        let source = project.source(path)?;
        Frontmatter::check(&source, path)?;
        let origin = Origin::new(&source, path, collection);
        let hash = Hash::of_bytes(source.text().as_bytes());
        let (frontmatter, export) = if self.enabled {
            let (module, deps, clock) = project.module_tracked(&source)?;
            let extracted = Self::interpret(&module, &origin, config)?;
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
    /// fence opened, as the dict every page's template receives, and its body
    /// as the Typst it compiles as.
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

        let block = document.block(&named, &text)?;
        let dict = block.dict;
        let origin = Origin::block(&text, &block.spans, path, collection);
        let frontmatter = Frontmatter::from_dict(&dict, &origin, config)?;

        let value = crate::codegen::Value::from(&typst::foundations::Value::Dict(dict));
        let dict = crate::codegen::Typst(&value).to_string();

        let sourced = Self::sourced(&frontmatter, &document, path, config, &origin)?;
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
    /// names each, beside the reader that turns the file into a body. The row
    /// is the dispatch, so adding a dialect is a row here and a [`Sourced`]
    /// variant.
    #[cfg(feature = "markdown")]
    const READERS: &'static [(&'static str, Reader)] = &[
        (Config::MARKDOWN, |_, _, file, text| Sourced::Markdown {
            named: file.display().to_string(),
            text,
        }),
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

    /// The file a page's `source` names, read. The name is resolved against
    /// `paths { sources { } }` and nowhere else, so a page can only ever reach
    /// a file the config already offered it.
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
        let ext = declared.extension().and_then(|e| e.to_str()).unwrap_or("");
        let read = Self::READERS
            .iter()
            .find(|(named, _)| *named == ext)
            .map(|(_, read)| read)
            .ok_or_else(|| {
                crate::error::ContentError::source_unreadable(
                    path,
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

    fn roots(&self) -> Roots<'_> {
        self.analyzer.roots()
    }

    /// Decode file bytes to text exactly as typst does when it builds a
    /// `Source`: a leading UTF-8 BOM is stripped, nothing else is transformed.
    /// `None` for non-UTF-8 input, which routes the caller to the parse path.
    fn decode(bytes: &[u8]) -> Option<String> {
        const BOM: &[u8] = b"\xef\xbb\xbf";
        let rest = bytes.strip_prefix(BOM).unwrap_or(bytes);
        std::str::from_utf8(rest).ok().map(str::to_owned)
    }

    /// Read the `frontmatter` export from an evaluated module, defaulting when
    /// the module exports none.
    fn interpret(module: &Module, origin: &Origin, config: &Config) -> Result<(Frontmatter, bool)> {
        Frontmatter::extract(module, origin, config)?.map_or_else(
            || Ok((Frontmatter::default(), false)),
            |frontmatter| Ok((frontmatter, true)),
        )
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

    /// Fingerprint the inputs that change how a frontmatter is interpreted or
    /// judged: the taxonomy keys, the collection schemas and their globs, the
    /// renderer, `paths`, and the generated modules. The last two are what an
    /// evaluation can read that no per-page dependency will ever record.
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
        assert_eq!(decode(b"hello").as_deref(), Some("hello"));
        assert_eq!(decode(b"\xef\xbb\xbfhello").as_deref(), Some("hello"));
        assert_eq!(decode(b"a\r\nb\rc\n").as_deref(), Some("a\r\nb\rc\n"));
        assert_eq!(decode(b"\xef\xbb\xbf").as_deref(), Some(""));
        assert_eq!(decode(b"a\xef\xbb\xbfb").as_deref(), Some("a\u{feff}b"));
        assert_eq!(decode(b"\xff\xfe"), None);
    }

    /// A hit reuses a page's validation as much as its extraction, so the
    /// schema it was judged against and the glob that selected it are part of
    /// what makes the entry valid.
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
        assert_eq!(
            salt("content { collections { blog { sort \"date\" } } }"),
            salt("content { collections { blog { sort \"title\"; reverse #true } } }")
        );
    }

    /// A generated `@baudelaire/*` module is served from memory and resolves to
    /// no path, so no page can record one as a dependency and only the salt can
    /// notice it changed.
    #[test]
    fn the_salt_covers_the_generated_modules() {
        let config = crate::config::Config::parse("site \"s\"").expect("should parse");
        assert_ne!(
            DiscoveryCache::salt(&config, crate::graph::Hash::of_bytes(b"one")),
            DiscoveryCache::salt(&config, crate::graph::Hash::of_bytes(b"two"))
        );
    }

    /// Re-pointing a declared source at another file of the same kind changes
    /// neither the generated module nor any page's bytes, so only the salt is
    /// left to notice.
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
