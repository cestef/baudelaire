//! Resolution of source-relative links to permalinks.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use typst::syntax::{Source, ast};

use crate::codegen::Value;
use crate::content::{Data, Page};
use crate::graph::Hash;

/// A link that names a page of this site: which page, and what the author wrote
/// after it.
///
/// The one parsed form of an internal link. A raw `href` is split exactly once,
/// where it is resolved, and travels as this afterwards: the render pass, the
/// link graph, the deep-link check and the build manifest all read one value
/// rather than each splitting the string again on a rule of its own. They did,
/// and disagreed: the graph read `#install?x` as a section named `install?x`
/// while the anchor check read it as `install`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(from = "String", into = "String")]
pub struct Target {
    /// The page's permalink, `#fragment` and `?query` stripped: what identifies
    /// the page at the other end.
    page: String,
    /// The `#fragment` / `?query` the link carried, empty when it carried
    /// neither. Kept verbatim, because it is what the rewritten `href` has to
    /// say; read through [`Target::fragment`], never matched on again.
    tail: String,
}

impl Target {
    /// The link `raw` wrote, aimed at the page served at `page`.
    ///
    /// The two halves come from different places on purpose: resolution has
    /// just mapped a `.typ` source path to the permalink it is served at, and
    /// only the tail survives from what the author typed.
    fn new(page: &str, raw: &str) -> Self {
        Self {
            page: page.to_owned(),
            tail: super::Tail::of(raw).tail.to_owned(),
        }
    }

    /// The page this link names.
    pub fn page(&self) -> &str {
        &self.page
    }

    /// The heading id the link aimed at, without the `#`.
    ///
    /// `None` when it named the page rather than a section within it: a
    /// `?query` is not a section, and neither is a bare `#`. A `?query` written
    /// *after* the fragment is not part of it either, which is the rule the
    /// deep-link check had of its own and the link graph did not.
    pub fn fragment(&self) -> Option<&str> {
        let anchor = self.tail.strip_prefix('#')?;
        let anchor = anchor.split('?').next().unwrap_or(anchor);
        (!anchor.is_empty()).then_some(anchor)
    }
}

/// The URL this link is served at: the permalink it resolved to, carrying
/// whatever the author wrote after it. What goes back into the `href`, and the
/// spelling the build manifest stores.
impl std::fmt::Display for Target {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}{}", self.page, self.tail)
    }
}

/// A link already whole, split back into its parts: how a target comes out of
/// the build manifest, and how a page written as a URL rather than as a source
/// path is read.
impl From<&str> for Target {
    fn from(url: &str) -> Self {
        let split = super::Tail::of(url);
        Self {
            page: split.path.to_owned(),
            tail: split.tail.to_owned(),
        }
    }
}

impl From<String> for Target {
    fn from(url: String) -> Self {
        Self::from(url.as_str())
    }
}

impl From<Target> for String {
    fn from(target: Target) -> Self {
        target.to_string()
    }
}

/// The links one page's own content carries: this page's contribution to the
/// site's link graph, as the [`Target`]s it points at.
///
/// A set, so the order is the same on every build and the same link written
/// twice is one edge. A target keeps the section it aimed at, because a link
/// into a page's section says more than a link to the page: it is what lets the
/// page at the other end group who linked to *what*. Which paragraph they add up
/// to one edge from is [`Backlinks`]'s business, not this type's.
///
/// Only links written in the content tree are collected. A layout's nav, a
/// sidebar, and the prev/next pair are links every page carries by virtue of its
/// template, and counting them would make every page a neighbour of every other.
/// See [`crate::render::transform::rewrite`] for where that line is drawn.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Outbound(BTreeSet<Target>);

impl Outbound {
    /// The links of a page that has none: what a check reads for a build that
    /// never asked for the graph.
    pub const EMPTY: &'static Self = &Self(BTreeSet::new());

    /// Record a resolved link written on the page permalinked `from`. A page
    /// linking to itself is not an edge, whichever of its own sections it names:
    /// it says nothing a reader already on that page does not know.
    pub fn record(&mut self, target: Target, from: &str) {
        if target.page() != from {
            self.0.insert(target);
        }
    }

    /// The links this page carries, in a stable order.
    pub fn targets(&self) -> impl Iterator<Item = &Target> {
        self.0.iter()
    }

    /// The same links as the *pages* they name: what asking "is anything
    /// pointing at this page?" reads. A page named both plainly and by section
    /// appears twice, which no caller cares about and every caller would
    /// otherwise dedup for itself.
    pub fn pages(&self) -> impl Iterator<Item = &str> {
        self.targets().map(Target::page)
    }

    /// Whether the page links to nothing, so a manifest entry can leave the
    /// field out entirely.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// What a page's source *looks* like it links to: every string literal in it
    /// that resolves to a page, read the way the render pass reads a link.
    ///
    /// Only ever a guess, and only used as one: it is what a cold build predicts
    /// each page's backlinks from, before there is a rendered site to invert
    /// (see `engine::links::Graph`). Both kinds of wrong are harmless, because the
    /// graph the build actually produces is what every page is checked against
    /// and a page guessed wrongly is simply compiled again. A link built in a
    /// loop is missed; a `.typ` path written for some other reason is counted.
    ///
    /// Reading every string rather than the arguments of `link` calls is
    /// deliberate: it costs one walk, needs to know nothing about how a link is
    /// spelled, and cannot go stale when that changes.
    pub fn scanned(
        source: &Source,
        page: &Path,
        permalink: &str,
        links: &LinkMap,
        lang: Option<&str>,
    ) -> Self {
        let mut out = Self::default();
        let mut stack = vec![source.root()];
        while let Some(node) = stack.pop() {
            stack.extend(node.children());
            let Some(raw) = node.cast::<ast::Str>() else {
                continue;
            };
            if let Link::Resolved(target) = links.classify(&raw.get(), page, lang).link {
                out.record(target, permalink);
            }
        }
        out
    }
}

/// The site's link graph, inverted: for each page, the pages whose content
/// links to it.
///
/// [`Outbound`] is what one page contributes; this is what the whole set of them
/// adds up to, and what a template renders as "linked from". It exists only
/// after every page has rendered, which is why a page compiles against a
/// *predicted* one and is recompiled when the prediction turns out wrong (see
/// `engine::links::Graph`).
pub enum Backlinks {
    /// `links { backlinks }` is off. A page compiles with an empty set and
    /// records no digest, so nothing about the graph can ever invalidate it.
    Off,
    /// Sources keyed by the permalink they point at. A page absent from the map
    /// is linked from nowhere.
    On(BTreeMap<String, Vec<Backlink>>),
}

/// One inbound link, as a template renders it: a page, once, whichever of this
/// page's sections it pointed at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Backlink {
    /// The permalink of the page that links here.
    pub url: String,
    pub title: String,
    /// Its language, so a template can drop or mark a link from another edition
    /// of the site: an `en` page linking a `fr` one is a real edge.
    pub lang: String,
    /// The heading ids it aimed at, without the `#`, in a stable order. Empty
    /// when it linked to the page rather than into it, which is what lets a
    /// template group its backlinks by section and still name a source once.
    pub fragments: Vec<String>,
}

impl Backlinks {
    /// Invert `edges`: each page's own outbound links become, for every page
    /// they name, the pages that name it.
    ///
    /// A source appears once per page it links to, however many links it wrote
    /// and however many sections they aimed at: those collect into
    /// [`Backlink::fragments`] instead, so the plain "linked from" list never
    /// says one name twice. Sources come ordered by permalink, so the value a
    /// page compiles against is the same on every build: an order that depended
    /// on which pages hit the cache would refingerprint pages that nothing
    /// changed about.
    pub fn new<'a>(edges: impl Iterator<Item = (&'a Page, &'a Outbound)>) -> Self {
        let mut inverted: BTreeMap<String, BTreeMap<String, Backlink>> = BTreeMap::new();
        for (page, outbound) in edges {
            for target in outbound.targets() {
                let source = inverted
                    .entry(target.page().to_owned())
                    .or_default()
                    .entry(page.permalink.clone())
                    .or_insert_with(|| Backlink::from(page));
                if let Some(anchor) = target.fragment() {
                    source.fragments.push(anchor.to_owned());
                }
            }
        }
        Self::On(
            inverted
                .into_iter()
                .map(|(target, sources)| (target, sources.into_values().collect()))
                .collect(),
        )
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
    /// `(url, title, lang, fragments)` dicts, `page.backlinks`.
    pub fn value(&self, page: &Page) -> Value {
        Value::array(self.of(page).iter().map(|source| {
            Value::dict([
                ("url", Value::str(&source.url)),
                ("title", Value::str(&source.title)),
                ("lang", Value::str(&source.lang)),
                (
                    "fragments",
                    Value::array(source.fragments.iter().map(Value::str)),
                ),
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
/// is, what to call it, and what language it is in. What it aimed at is the
/// link's, not the page's, and is filled in as the graph is inverted.
impl From<&Page> for Backlink {
    fn from(page: &Page) -> Self {
        Self {
            url: page.permalink.clone(),
            title: page.frontmatter.title.clone().unwrap_or_default(),
            lang: page.lang.clone(),
            fragments: Vec::new(),
        }
    }
}

/// How a raw link in page markup should be treated.
#[derive(Debug, PartialEq, Eq)]
pub enum Link {
    /// Not a managed page link (external, fragment, or non-`.typ`); leave as authored.
    Passthrough,
    /// An internal `.typ` link resolved to the page it names.
    Resolved(Target),
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
    /// Every permalink this site serves, so a link written as a URL rather than
    /// as a source path can still be recognized as naming a page. Generated
    /// listings are in here even though they are not in `by_source`: a link to a
    /// term page is a link to a page, whoever wrote it.
    urls: HashSet<String>,
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
            .filter(|p| !matches!(p.data, Data::Generated { .. }))
            .map(|p| (crate::fs::resolved(&p.source), p.permalink.clone()))
            .collect();
        Self {
            by_source,
            urls: pages.iter().map(|p| p.permalink.clone()).collect(),
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

    /// The page a raw link already spelled as a URL names, if this site serves
    /// one there.
    ///
    /// A `.typ` link is the way to cross-reference a page and the only spelling
    /// that survives a rename, but it is not the only one that *reaches* a page:
    /// an author writing `/guide/` means the guide, and a generated index links
    /// its members by permalink because it has no source path to name them by.
    /// Both are edges of the link graph, and neither goes through
    /// [`LinkMap::classify`], which exists to rewrite what has to be rewritten.
    pub fn served(&self, raw: &str) -> Option<Target> {
        let target = Target::from(raw);
        self.urls.contains(target.page()).then_some(target)
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
            Some(permalink) => Link::Resolved(Target::new(&permalink, raw)),
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
    use super::{Backlinks, LinkMap, Outbound, Source, Target};
    use crate::content::{Data, Frontmatter, Page, PageId, Siblings};
    use std::path::PathBuf;

    /// One rule for what a link aimed at, read once. The graph and the anchor
    /// check each had their own and disagreed about `#install?x`; a link is also
    /// served back exactly as it was written, query and all.
    #[test]
    fn a_target_names_one_page_and_at_most_one_section() {
        for (raw, page, fragment) in [
            ("/b/", "/b/", None),
            ("/b/#install", "/b/", Some("install")),
            ("/b/?x=1", "/b/", None),
            ("/b/#", "/b/", None),
            ("/b/#install?x=1", "/b/", Some("install")),
        ] {
            let target = Target::from(raw);
            assert_eq!(
                (target.page(), target.fragment()),
                (page, fragment),
                "{raw}"
            );
            assert_eq!(target.to_string(), raw);
        }
    }

    /// A target is stored as the URL it names and nothing else, which is what
    /// lets it be parsed without a [`crate::graph::Renderer::SCHEMA`] bump: a
    /// manifest written when these were plain strings reads back identically,
    /// and one written now is still readable by a build that has not upgraded.
    #[test]
    fn a_target_is_stored_as_the_url_it_names() {
        let mut outbound = Outbound::default();
        outbound.record(Target::from("/b/#install"), "/a/");
        outbound.record(Target::from("/c/"), "/a/");

        let json = serde_json::to_string(&outbound).unwrap();

        assert_eq!(json, r#"["/b/#install","/c/"]"#);
        assert_eq!(
            serde_json::from_str::<Outbound>(&json).unwrap(),
            outbound,
            "a stored target round-trips"
        );
    }

    /// A link is kept whole, so the page at the other end can tell which of its
    /// sections was aimed at. The same link written twice is still one edge, and
    /// a page never links to itself, whichever of its own sections it names.
    #[test]
    fn an_edge_keeps_what_the_link_aimed_at() {
        let mut outbound = Outbound::default();
        for target in [
            "/posts/b/#install",
            "/posts/b/#install",
            "/posts/b/",
            "/posts/a/#top",
        ] {
            outbound.record(Target::from(target), "/posts/a/");
        }
        assert_eq!(urls(&outbound), ["/posts/b/", "/posts/b/#install"]);
    }

    /// Inverting names each source once per page it links to, however many links
    /// it wrote: the sections they aimed at collect on that one entry, which is
    /// what lets a template group by section without listing a page twice.
    #[test]
    fn a_source_is_named_once_and_carries_the_sections_it_aimed_at() {
        let mut outbound = Outbound::default();
        for target in ["/posts/b/", "/posts/b/#install", "/posts/b/#usage"] {
            outbound.record(Target::from(target), "/posts/a/");
        }
        let (source, target) = (page("A", "/posts/a/"), page("B", "/posts/b/"));
        let backlinks = Backlinks::new(std::iter::once((&source, &outbound)));

        let sources = backlinks.of(&target);
        assert_eq!(sources.len(), 1);
        assert_eq!(sources[0].url, "/posts/a/");
        assert_eq!(sources[0].title, "A");
        assert_eq!(sources[0].fragments, ["install", "usage"]);
        // A page nothing points at is linked from nowhere, not from everywhere.
        assert!(backlinks.of(&source).is_empty());
    }

    /// The links a page carries, as the URLs they are served at: what an
    /// assertion about a page's edges reads.
    fn urls(outbound: &Outbound) -> Vec<String> {
        outbound.targets().map(Target::to_string).collect()
    }

    /// The pages a backlink test links between: a title and a permalink are all
    /// an inbound link is made of.
    fn page(title: &str, permalink: &str) -> Page {
        Page {
            id: PageId::new("posts", title),
            source: PathBuf::from(format!("content/{}.typ", title.to_lowercase())),
            frontmatter: Frontmatter {
                title: Some(title.to_owned()),
                ..Frontmatter::default()
            },
            body: String::new(),
            data: Data::Empty,
            collection: "posts".into(),
            permalink: permalink.to_owned(),
            output: PathBuf::new(),
            template: None,
            lang: "en".into(),
            siblings: Siblings::default(),
            translations: Vec::new(),
        }
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

    /// A link written as a URL names a page too, which is how a generated index
    /// reaches its members: it has no source path to name them by.
    #[test]
    fn a_link_spelled_as_a_url_still_names_a_page() {
        let pages = [page("A", "/a/"), page("B", "/b/")];
        let map = LinkMap::new(&pages, std::path::Path::new("."));

        let page = |raw| map.served(raw).map(|t| t.page().to_owned());
        assert_eq!(page("/b/").as_deref(), Some("/b/"));
        // The section is not part of the page's identity, and a page this site
        // does not serve is not one of ours.
        assert_eq!(page("/b/#install").as_deref(), Some("/b/"));
        assert_eq!(page("/nowhere/"), None);
        assert_eq!(page("https://example.com/b/"), None);
    }

    /// The cold-build guess: what a page's source looks like it links to,
    /// resolved through the same map the render pass uses. Only a guess, so it
    /// is allowed to be wide; it must not be *wrong* about what resolves.
    #[test]
    fn a_scan_finds_the_links_a_source_writes_out() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        std::fs::create_dir_all(root.join("content")).unwrap();
        for name in ["a.typ", "b.typ"] {
            std::fs::write(root.join("content").join(name), "").unwrap();
        }
        let pages = [page("A", "/a/"), page("B", "/b/")];
        let map = LinkMap::new(&pages, root);
        let text = r#"#link("b.typ")[to b] #link("a.typ")[self] #image("photo.png") "b.typ""#;
        let source = Source::detached(text);

        let scanned = Outbound::scanned(&source, &pages[0].source, &pages[0].permalink, &map, None);

        // The link to b, once, however many strings named it; the self-link and
        // the non-page reference are not edges.
        assert_eq!(urls(&scanned), ["/b/"]);
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
