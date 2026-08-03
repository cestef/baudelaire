//! Resolution of source-relative links to permalinks.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::codegen::Value;
use crate::content::{Data, Page};
use crate::graph::Hash;

/// The pages one page's own content links to: this page's contribution to the
/// site's link graph, as permalinks.
///
/// A set, so the order is the same on every build and two links to one page are
/// one edge. What a link addresses *within* a page (`#fragment`, `?query`) is
/// dropped: the edge names the page, not the paragraph.
///
/// Only links written in the content tree are collected. A layout's nav, a
/// sidebar, and the prev/next pair are links every page carries by virtue of its
/// template, and counting them would make every page a neighbour of every other.
/// See [`crate::render::transform::rewrite`] for where that line is drawn.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Outbound(BTreeSet<String>);

impl Outbound {
    /// Record a resolved link written on the page permalinked `from`. A page
    /// linking to itself is not an edge: it says nothing a reader of that page
    /// does not already know.
    pub fn record(&mut self, url: &str, from: &str) {
        let target = super::Tail::of(url).path;
        if target != from {
            self.0.insert(target.to_owned());
        }
    }

    /// The permalinks this page links to, in a stable order.
    pub fn targets(&self) -> impl Iterator<Item = &str> {
        self.0.iter().map(String::as_str)
    }

    /// Whether the page links to nothing, so a manifest entry can leave the
    /// field out entirely.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

}

/// The site's link graph, inverted: for each page, the pages whose content
/// links to it.
///
/// [`Outbound`] is what one page contributes; this is what the whole set of them
/// adds up to, and what a template renders as "linked from". It exists only
/// after every page has rendered, which is why a page compiles against a
/// *predicted* one and is recompiled when the prediction turns out wrong (see
/// `Engine::backlinks`).
pub enum Backlinks {
    /// `links { backlinks }` is off. A page compiles with an empty set and
    /// records no digest, so nothing about the graph can ever invalidate it.
    Off,
    /// Sources keyed by the permalink they point at. A page absent from the map
    /// is linked from nowhere.
    On(BTreeMap<String, Vec<Backlink>>),
}

/// One inbound link, as a template renders it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Backlink {
    /// The permalink of the page that links here.
    pub url: String,
    pub title: String,
    /// Its language, so a template can drop or mark a link from another edition
    /// of the site: an `en` page linking a `fr` one is a real edge.
    pub lang: String,
}

impl Backlinks {
    /// Invert `edges` (each page's own outbound links) over `pages`.
    ///
    /// Sources are ordered by permalink, so the value a page compiles against is
    /// the same on every build: an order that depended on which pages hit the
    /// cache would refingerprint pages that nothing changed about.
    pub fn new<'a>(edges: impl Iterator<Item = (&'a Page, &'a Outbound)>) -> Self {
        let mut inverted: BTreeMap<String, Vec<Backlink>> = BTreeMap::new();
        for (page, outbound) in edges {
            for target in outbound.targets() {
                inverted
                    .entry(target.to_owned())
                    .or_default()
                    .push(Backlink::from(page));
            }
        }
        for sources in inverted.values_mut() {
            sources.sort_by(|a, b| a.url.cmp(&b.url));
        }
        Self::On(inverted)
    }

    /// The pages linking to `page`, empty when none do or when the feature is
    /// off.
    pub fn of(&self, page: &Page) -> &[Backlink] {
        match self {
            Self::Off => &[],
            Self::On(inverted) => inverted
                .get(&page.permalink)
                .map_or(&[], |sources| sources.as_slice()),
        }
    }

    /// What a page's backlinks are handed to its template as: an array of
    /// `(url, title, lang)` dicts, `page.backlinks`.
    pub fn value(&self, page: &Page) -> Value {
        Value::array(self.of(page).iter().map(|source| {
            Value::dict([
                ("url", Value::str(&source.url)),
                ("title", Value::str(&source.title)),
                ("lang", Value::str(&source.lang)),
            ])
        }))
    }

    /// The digest of what `page` was, or would be, compiled with. `None` when
    /// the feature is off: there is then nothing to validate a page against, and
    /// a recorded digest would only force a rebuild when it was turned on.
    pub fn digest(&self, page: &Page) -> Option<Hash> {
        match self {
            Self::Off => None,
            Self::On(_) => Some(Hash::of(&self.value(page))),
        }
    }
}

/// A page names itself in a backlink by the three things a link needs: where it
/// is, what to call it, and what language it is in.
impl From<&Page> for Backlink {
    fn from(page: &Page) -> Self {
        Self {
            url: page.permalink.clone(),
            title: page.frontmatter.title.clone().unwrap_or_default(),
            lang: page.lang.clone(),
        }
    }
}

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
        // Case-sensitively, the way discovery matches content files: typst
        // resolves the path literally, so `b.TYP` is not a link to `b.typ`.
        if Path::new(split.path).extension().is_none_or(|e| e != "typ") {
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
    use super::{LinkMap, Outbound};

    /// An edge names a page, so two links to one page (or to two of its
    /// sections) are one edge, and a page never links to itself.
    #[test]
    fn an_edge_names_a_page_once() {
        let mut outbound = Outbound::default();
        outbound.record("/posts/b/#install", "/posts/a/");
        outbound.record("/posts/b/?utm=x", "/posts/a/");
        outbound.record("/posts/b/", "/posts/a/");
        outbound.record("/posts/c/", "/posts/a/");
        outbound.record("/posts/a/#top", "/posts/a/");
        assert_eq!(
            outbound.targets().collect::<Vec<_>>(),
            ["/posts/b/", "/posts/c/"]
        );
    }

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
