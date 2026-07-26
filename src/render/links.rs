//! Resolution of source-relative links to permalinks.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use crate::content::Page;
use crate::graph::{Fingerprint, Hash};

/// How a raw link in page markup should be treated.
#[derive(Debug, PartialEq, Eq)]
pub enum Link {
    /// Not a managed page link (external, fragment, or non-`.typ`); leave as authored.
    Passthrough,
    /// An internal `.typ` link resolved to this URL (permalink + any `#frag`/`?query`).
    Resolved(String),
    /// An internal `.typ` link whose target page does not exist.
    Broken,
}

/// Maps content source files to their resolved permalinks. Links written
/// against `.typ` source paths (the typst-native way to cross-reference pages)
/// resolve to the target page's clean URL, so links survive permalink changes.
#[derive(Debug, Default)]
pub struct LinkMap {
    by_source: HashMap<PathBuf, String>,
    /// The typst project root: absolute link paths (`/posts/hello.typ`)
    /// resolve against it, mirroring typst's own path convention.
    root: PathBuf,
}

impl Fingerprint for LinkMap {
    /// A stable fingerprint of every source-to-permalink mapping. Folded into the
    /// build cache so a page whose permalink changed invalidates the cached
    /// pages that might link to it: link resolution is render-side and so is
    /// invisible to the per-page dependency tracker, which never sees a link
    /// target's source. Coarse (any permalink change rebuilds every page), but
    /// permalink changes are rare and this mirrors how the asset map is folded
    /// in.
    fn fingerprint(&self) -> Hash {
        // Keyed by path *relative to the project root*. The map itself indexes
        // by canonical absolute path (that is what a resolved link produces),
        // but folding those in would tie the cache fingerprint to where the
        // site happens to sit, so a warm cache would miss entirely after
        // `mv site site2` or a CI checkout at a different workspace path.
        let sorted: BTreeMap<&Path, &String> = self
            .by_source
            .iter()
            .map(|(source, permalink)| {
                (source.strip_prefix(&self.root).unwrap_or(source), permalink)
            })
            .collect();
        Hash::of(&sorted)
    }
}

impl LinkMap {
    /// Index every page by the canonical path of its source file. `root` is
    /// the typst project root absolute references resolve against.
    pub fn new(pages: &[Page], root: &Path) -> Self {
        let by_source = pages
            .iter()
            .filter_map(|p| {
                Some((
                    crate::fs::canonicalize(&p.source).ok()?,
                    p.permalink.clone(),
                ))
            })
            .collect();
        Self {
            by_source,
            root: root.to_path_buf(),
        }
    }

    /// Classify a raw link written in `from`'s body: passthrough, resolved to a
    /// permalink, or a broken internal `.typ` reference.
    ///
    /// `lang` is the linking page's language on a multilingual site, `None`
    /// otherwise. A translated page writes the same `#link("b.typ")` as its
    /// original, and means its own edition of `b`; resolving language-blind sent
    /// every French link to the English page, silently and with no warning,
    /// since the link did resolve.
    pub fn classify(&self, raw: &str, from: &Path, lang: Option<&str>) -> Link {
        if Self::is_external(raw) {
            return Link::Passthrough;
        }
        let split = super::Tail::of(raw);
        if !split.path.ends_with(".typ") {
            return Link::Passthrough;
        }
        // Typst path semantics: absolute paths are project-root-relative,
        // relative ones resolve against the linking file's directory.
        let target = match split.path.strip_prefix('/') {
            Some(rooted) => self.root.join(rooted),
            None => from
                .parent()
                .unwrap_or_else(|| Path::new("."))
                .join(split.path),
        };
        let edition = lang.and_then(|lang| self.edition(&target, lang));
        match edition.or_else(|| self.lookup(&target)) {
            Some(permalink) => Link::Resolved(format!("{permalink}{}", split.tail)),
            None => Link::Broken,
        }
    }

    /// The `{stem}.{lang}.typ` sibling of `target`, when the site has one.
    fn edition(&self, target: &Path, lang: &str) -> Option<&String> {
        let stem = target.file_stem()?.to_str()?;
        self.lookup(&target.with_file_name(format!("{stem}.{lang}.typ")))
    }

    /// The permalink of the page whose source is `path`.
    fn lookup(&self, path: &Path) -> Option<&String> {
        self.by_source.get(&crate::fs::canonicalize(path).ok()?)
    }

    /// Whether a link points outside the site (scheme, protocol-relative,
    /// mailto, or a bare fragment) and must be left as authored.
    fn is_external(raw: &str) -> bool {
        raw.starts_with("//")
            || raw.starts_with('#')
            || raw.starts_with("mailto:")
            || raw.contains("://")
    }
}

#[cfg(test)]
mod tests {
    use super::LinkMap;

    #[test]
    fn external_links_are_recognized() {
        for raw in [
            "https://example.com",
            "http://x",
            "//cdn",
            "mailto:a@b.c",
            "#anchor",
        ] {
            assert!(LinkMap::is_external(raw), "{raw} should be external");
        }
    }

    #[test]
    fn local_links_are_not_external() {
        for raw in ["b.typ", "../notes/x.typ", "b.typ#section"] {
            assert!(!LinkMap::is_external(raw), "{raw} should be local");
        }
    }

    #[test]
    fn splits_fragment_and_query() {
        for (raw, path, tail) in [
            ("b.typ", "b.typ", ""),
            ("b.typ#s", "b.typ", "#s"),
            ("b.typ?x=1", "b.typ", "?x=1"),
        ] {
            let split = crate::render::Tail::of(raw);
            assert_eq!((split.path, split.tail), (path, tail));
        }
    }

    #[test]
    fn unknown_typ_target_is_broken_external_is_passthrough() {
        use super::Link;
        let map = LinkMap::default();
        let from = std::path::Path::new("a.typ");
        assert_eq!(map.classify("missing.typ", from, None), Link::Broken);
        assert_eq!(map.classify("https://x.com", from, None), Link::Passthrough);
        assert_eq!(map.classify("#section", from, None), Link::Passthrough);
        assert_eq!(
            map.classify("/already/a/url/", from, None),
            Link::Passthrough
        );
    }
}
