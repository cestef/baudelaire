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
    /// A stable fingerprint of every source→permalink mapping. Folded into the
    /// build cache so a page whose permalink changed invalidates the cached
    /// pages that might link to it — link resolution is render-side and so is
    /// invisible to the per-page dependency tracker, which never sees a link
    /// target's source. Coarse (any permalink change rebuilds every page), but
    /// permalink changes are rare and this mirrors how the asset map is folded
    /// in.
    fn fingerprint(&self) -> Hash {
        let sorted: BTreeMap<&PathBuf, &String> = self.by_source.iter().collect();
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
    pub fn classify(&self, raw: &str, from: &Path) -> Link {
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
        match crate::fs::canonicalize(target)
            .ok()
            .and_then(|canon| self.by_source.get(&canon))
        {
            Some(permalink) => Link::Resolved(format!("{permalink}{}", split.tail)),
            None => Link::Broken,
        }
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
        assert_eq!(map.classify("missing.typ", from), Link::Broken);
        assert_eq!(map.classify("https://x.com", from), Link::Passthrough);
        assert_eq!(map.classify("#section", from), Link::Passthrough);
        assert_eq!(map.classify("/already/a/url/", from), Link::Passthrough);
    }
}
