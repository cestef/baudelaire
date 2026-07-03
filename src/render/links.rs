//! Resolution of source-relative links to permalinks.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::content::Page;

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
}

impl LinkMap {
    /// Index every page by the canonical path of its source file.
    pub fn new(pages: &[Page]) -> Self {
        let by_source = pages
            .iter()
            .filter_map(|p| Some((p.source.canonicalize().ok()?, p.permalink.clone())))
            .collect();
        Self { by_source }
    }

    /// Classify a raw link written in `from`'s body: passthrough, resolved to a
    /// permalink, or a broken internal `.typ` reference.
    pub fn classify(&self, raw: &str, from: &Path) -> Link {
        if Self::is_external(raw) {
            return Link::Passthrough;
        }
        let (path, tail) = Self::split_tail(raw);
        if !path.ends_with(".typ") {
            return Link::Passthrough;
        }
        let base = from.parent().unwrap_or_else(|| Path::new("."));
        match base
            .join(path)
            .canonicalize()
            .ok()
            .and_then(|canon| self.by_source.get(&canon))
        {
            Some(permalink) => Link::Resolved(format!("{permalink}{tail}")),
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

    /// Split a link into its path and its trailing `#fragment` / `?query`.
    fn split_tail(raw: &str) -> (&str, &str) {
        match raw.find(['#', '?']) {
            Some(i) => raw.split_at(i),
            None => (raw, ""),
        }
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
        assert_eq!(LinkMap::split_tail("b.typ"), ("b.typ", ""));
        assert_eq!(LinkMap::split_tail("b.typ#s"), ("b.typ", "#s"));
        assert_eq!(LinkMap::split_tail("b.typ?x=1"), ("b.typ", "?x=1"));
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
