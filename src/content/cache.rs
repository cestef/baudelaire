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

use crate::config::Config;
use crate::content::Frontmatter;
use crate::error::{Artifact, Result, SerializeError};
use crate::graph::{FileDigests, Hash, Renderer};
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
    deps: BTreeMap<PathBuf, Hash>,
    /// The extracted frontmatter.
    frontmatter: Frontmatter,
    /// Whether the module exported a `frontmatter` binding (vs. defaulted).
    export: bool,
}

/// The serialized discovery manifest.
#[derive(Debug, Default, Serialize, Deserialize)]
struct Manifest {
    /// Fingerprint of the config inputs that change how frontmatter is
    /// interpreted (the configured taxonomy keys). A change invalidates every
    /// entry: a key that was `extra` yesterday may be a taxonomy today.
    salt: Option<Hash>,
    /// Entries keyed by page source path.
    pages: BTreeMap<PathBuf, Entry>,
}

/// Persisted extracted frontmatter, so discovery skips the typst module
/// evaluation for pages whose source and dependencies are unchanged.
pub struct DiscoveryCache {
    dir: PathBuf,
    enabled: bool,
    salt: Hash,
    prev: Manifest,
    /// The manifest being accumulated this build. Filled during the parallel
    /// page load, hence the lock; contention is negligible (a map insert).
    next: Mutex<Manifest>,
    /// Per-build file-hash memo: a module imported by many pages is hashed once
    /// while validating their dependencies, not once per page.
    digests: FileDigests,
}

impl DiscoveryCache {
    /// Load the cache for a build. When incremental builds are disabled it never
    /// reports a hit and never persists: every page evaluates live.
    pub fn load(config: &Config) -> Self {
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
        path: &Path,
        config: &Config,
        project: &Project,
    ) -> Result<(Frontmatter, bool, String)> {
        // Fast path: unchanged source and dependencies reuse the stored
        // frontmatter with no typst parse or evaluation at all.
        if self.enabled
            && let Some(body) = Self::decode(&crate::fs::read(path)?)
        {
            let hash = Hash::of_bytes(body.as_bytes());
            if let Some(entry) = self.reuse(path, hash) {
                return Ok((entry.frontmatter, entry.export, body));
            }
        }
        // Miss (or caching disabled, or non-UTF-8): parse and evaluate the page.
        let source = project.source(path)?;
        Frontmatter::check(&source, path)?;
        let hash = Hash::of_bytes(source.text().as_bytes());
        let (frontmatter, export) = if self.enabled {
            let (module, deps) = project.module_tracked(&source)?;
            let extracted = Self::interpret(&module, path, config)?;
            let deps = deps
                .files()
                .iter()
                .filter_map(|p| Some((p.clone(), self.digests.of(p)?)))
                .collect();
            self.next.lock().pages.insert(
                path.to_owned(),
                Entry {
                    source: hash,
                    deps,
                    frontmatter: extracted.0.clone(),
                    export: extracted.1,
                },
            );
            extracted
        } else {
            Self::interpret(&project.module(&source)?, path, config)?
        };
        Ok((frontmatter, export, source.text().to_owned()))
    }

    /// The previous entry for `path` if it is still valid, its source and every
    /// dependency hash unchanged, carried into the next manifest so it survives
    /// to the following build.
    fn reuse(&self, path: &Path, hash: Hash) -> Option<Entry> {
        let entry = self.prev.pages.get(path)?;
        if entry.source != hash {
            return None;
        }
        if !entry
            .deps
            .iter()
            .all(|(p, h)| self.digests.of(p).as_ref() == Some(h))
        {
            return None;
        }
        self.next
            .lock()
            .pages
            .insert(path.to_owned(), entry.clone());
        Some(entry.clone())
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
    fn interpret(module: &Module, path: &Path, config: &Config) -> Result<(Frontmatter, bool)> {
        match Frontmatter::extract(module, path, config)? {
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
    fn salt(config: &Config) -> Hash {
        let keys: Vec<&str> = config
            .content
            .taxonomies
            .iter()
            .map(|(_, t)| t.key.as_str())
            .collect();
        Hash::of(&(keys, Renderer::current()))
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
}
