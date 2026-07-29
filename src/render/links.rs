//! Resolution of source-relative links to permalinks.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use crate::content::{Data, Page};

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

/// The link-map entries a page's resolution consulted: for each source path
/// probed, the permalink it mapped to, or `None` when no page sat there.
///
/// This is a page's dependency on the site's URL layout, which the per-page
/// dependency tracker cannot see (resolution is render-side, and typst never
/// reads a link target's source). Recorded per page and revalidated on a cache
/// hit, so a permalink change invalidates the pages that linked to it rather
/// than the whole site.
///
/// A `None` carries as much weight as a `Some`: a link that fell through to the
/// base page because the site had no `.de` edition of it must rebuild when that
/// edition appears. Recording only the entry that matched would leave it stale.
pub type LinkDeps = BTreeMap<PathBuf, Option<String>>;

/// How one raw link resolved, and the map entries the outcome depended on.
///
/// The two travel together because they are computed together: deriving the
/// dependencies in a second pass would mean spelling the probe order twice.
pub struct Resolution {
    /// What to do with the link.
    pub link: Link,
    /// Every entry consulted before settling on [`Resolution::link`].
    pub probed: LinkDeps,
}

impl Resolution {
    /// A link that is none of this module's business, and so depends on nothing.
    fn passthrough() -> Self {
        Self {
            link: Link::Passthrough,
            probed: LinkDeps::new(),
        }
    }
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

impl LinkMap {
    /// Index every page by the resolved path of its source file, the spelling
    /// [`LinkMap::candidates`] probes with. `root` is the typst project root
    /// absolute references resolve against.
    ///
    /// Generated listings are excluded: their source path is fabricated and no
    /// file sits there, so no author can write a `.typ` link against it. Said
    /// outright rather than left to canonicalization failing on a path that is
    /// not there, because resolution has to give the same answer for a target
    /// that exists and one that does not.
    pub fn new(pages: &[Page], root: &Path) -> Self {
        let by_source = pages
            .iter()
            .filter(|p| !matches!(p.data, Data::Generated(_)))
            .map(|p| (crate::fs::resolved(&p.source), p.permalink.clone()))
            .collect();
        Self {
            by_source,
            root: root.to_path_buf(),
        }
    }

    /// Every indexed page as `(source path, permalink)`.
    ///
    /// The build cache keeps its own copy, keyed the way it stores every other
    /// path, so it can revalidate a page's recorded [`LinkDeps`] without
    /// reaching into this map's internals or repeating its normalization.
    pub fn entries(&self) -> impl Iterator<Item = (&Path, &str)> {
        self.by_source
            .iter()
            .map(|(source, permalink)| (source.as_path(), permalink.as_str()))
    }

    /// Classify a raw link written in `from`'s body: passthrough, resolved to a
    /// permalink, or a broken internal `.typ` reference, together with the map
    /// entries that decided it.
    ///
    /// `lang` is the linking page's language on a multilingual site, `None`
    /// otherwise. A translated page writes the same `#link("b.typ")` as its
    /// original, and means its own edition of `b`; resolving language-blind sent
    /// every French link to the English page, silently and with no warning,
    /// since the link did resolve.
    pub fn classify(&self, raw: &str, from: &Path, lang: Option<&str>) -> Resolution {
        if Self::is_external(raw) {
            return Resolution::passthrough();
        }
        let split = super::Tail::of(raw);
        if !split.path.ends_with(".typ") {
            return Resolution::passthrough();
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
        // Probing stops at the first candidate that exists, so only the entries
        // actually consulted become dependencies: a resolved edition means the
        // base page's permalink never mattered and must not invalidate.
        let mut probed = LinkDeps::new();
        let resolved = Self::candidates(&target, lang).find_map(|candidate| {
            let permalink = self.by_source.get(&candidate).cloned();
            probed.insert(candidate, permalink.clone());
            permalink
        });
        let link = match resolved {
            Some(permalink) => Link::Resolved(format!("{permalink}{}", split.tail)),
            None => Link::Broken,
        };
        Resolution { link, probed }
    }

    /// The source paths a link to `target` probes, in order: the reader's own
    /// language edition first, then the target as written. Spelled by
    /// [`crate::fs::resolved`], the same rule [`LinkMap::new`] indexes by, so
    /// they key into [`LinkMap::by_source`] and compare equal across builds.
    ///
    /// The spelling has to hold for a target that is *not* there, since a probe
    /// resolving to nothing is recorded as a dependency ("no page sat here") and
    /// revalidated against the page that later appears at that path.
    /// [`crate::fs::canonical`] would spell those two differently the moment any
    /// ancestor is a symlink, leaving the recorded absence matching forever and
    /// the linking page cached with a broken link.
    fn candidates(target: &Path, lang: Option<&str>) -> impl Iterator<Item = PathBuf> {
        lang.and_then(|lang| Self::edition(target, lang))
            .into_iter()
            .chain(std::iter::once(target.to_path_buf()))
            .map(crate::fs::resolved)
    }

    /// The `{stem}.{lang}.typ` sibling of `target`: the reader's own edition of
    /// the page a link points at.
    fn edition(target: &Path, lang: &str) -> Option<PathBuf> {
        let stem = target.file_stem()?.to_str()?;
        Some(target.with_file_name(format!("{stem}.{lang}.typ")))
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
        assert_eq!(map.classify("missing.typ", from, None).link, Link::Broken);
        for raw in ["https://x.com", "#section", "/already/a/url/"] {
            assert_eq!(
                map.classify(raw, from, None).link,
                Link::Passthrough,
                "{raw} should pass through"
            );
        }
    }

    #[test]
    fn a_link_that_resolves_to_nothing_still_records_what_it_probed() {
        // The negative dependency: without it, the page that links to a
        // not-yet-written target stays cached when that target appears, and
        // serves a link that is still broken.
        let map = LinkMap::default();
        let from = std::path::Path::new("a.typ");

        let probed = map.classify("missing.typ", from, None).probed;

        assert_eq!(probed.len(), 1, "{probed:?}");
        assert!(
            probed.values().all(Option::is_none),
            "an unresolved probe maps to nothing: {probed:?}"
        );
    }

    #[test]
    fn a_passthrough_link_depends_on_nothing() {
        let map = LinkMap::default();
        let from = std::path::Path::new("a.typ");

        assert!(map.classify("https://x.com", from, None).probed.is_empty());
    }

    #[test]
    fn a_multilingual_link_probes_the_edition_before_the_target() {
        let map = LinkMap::default();
        let from = std::path::Path::new("a.typ");

        let probed = map.classify("b.typ", from, Some("de")).probed;

        // Both, because neither exists: the edition is consulted first, and the
        // fall-through to the base page is only reached because it was absent.
        let names: Vec<_> = probed
            .keys()
            .filter_map(|p| p.file_name()?.to_str())
            .collect();
        assert_eq!(names, ["b.de.typ", "b.typ"], "{probed:?}");
    }
}
