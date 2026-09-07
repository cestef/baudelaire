//! Resolution of source-relative links to permalinks.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use typst::syntax::{Source, ast};

use crate::codegen::Value;
use crate::config::Scheme;
use crate::content::Page;
use crate::graph::Hash;

/// A link that names a page of this site: which page, and what the author wrote
/// after it.
///
/// The one parsed form of an internal link: an `href` is split exactly once, so
/// the render pass, the link graph and the deep-link check cannot disagree on
/// where the page ends and the fragment begins.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(from = "String", into = "String")]
pub struct Target {
    /// The page's permalink, `#fragment` and `?query` stripped.
    page: String,
    /// The `#fragment` / `?query` the link carried, empty when it carried
    /// neither, kept verbatim for the rewritten `href`.
    tail: String,
}

impl Target {
    /// The link `raw` wrote, aimed at the page served at `page`.
    fn new(page: &str, raw: &str) -> Self {
        Self {
            page: page.to_owned(),
            tail: super::Tail::of(raw).tail.to_owned(),
        }
    }

    pub fn page(&self) -> &str {
        &self.page
    }

    /// The heading id the link aimed at, without the `#`.
    ///
    /// Everything after the first `#`, since a `?` after it is part of the
    /// fragment a browser resolves. `None` when the link named the page rather
    /// than a section within it: a `?query` alone is not a section, and neither
    /// is a bare `#`.
    pub fn fragment(&self) -> Option<&str> {
        let anchor = self.tail.split_once('#')?.1;
        (!anchor.is_empty()).then_some(anchor)
    }
}

impl std::fmt::Display for Target {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}{}", self.page, self.tail)
    }
}

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

/// The links one page's own content carries, as the [`Target`]s it points at.
///
/// Only links written in the content tree are collected, never a template's nav
/// or prev/next pair: counting those would make every page a neighbour of every
/// other.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Outbound(BTreeSet<Target>);

impl Outbound {
    pub const EMPTY: &'static Self = &Self(BTreeSet::new());

    /// Record a resolved link written on the page permalinked `from`. A page
    /// linking to itself is not an edge, whichever of its own sections it
    /// names.
    pub fn record(&mut self, target: Target, from: &str) {
        if target.page() != from {
            self.0.insert(target);
        }
    }

    /// The links this page carries, in a stable order.
    pub fn targets(&self) -> impl Iterator<Item = &Target> {
        self.0.iter()
    }

    /// The same links as the *pages* they name; a page named both plainly and
    /// by section appears twice.
    pub fn pages(&self) -> impl Iterator<Item = &str> {
        self.targets().map(Target::page)
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// What a page's source *looks* like it links to: every string literal in
    /// it that resolves to a page, read the way the render pass reads a link.
    ///
    /// Only ever a guess, and only used as one: it is what a cold build
    /// predicts each page's backlinks from, before there is a rendered site to
    /// invert. Being wrong either way costs a recompile, since the graph the
    /// build produces is what every page is checked against.
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
pub enum Backlinks {
    /// `links { backlinks }` is off, so a page compiles with an empty set and
    /// records no digest.
    Off,
    /// Sources keyed by the permalink they point at. A page absent from the map
    /// is linked from nowhere.
    On(BTreeMap<String, Vec<Backlink>>),
}

/// One inbound link: a page, named once whichever of this page's sections it
/// pointed at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Backlink {
    /// The permalink of the page that links here.
    pub url: String,
    pub title: String,
    pub lang: String,
    /// The heading ids it aimed at, without the `#`, in a stable order; empty
    /// when it linked to the page rather than into it.
    pub fragments: Vec<String>,
}

impl Backlinks {
    /// Invert `edges`: each page's own outbound links become, for every page
    /// they name, the pages that name it.
    ///
    /// A source appears once per page it links to, the sections it aimed at
    /// collecting into [`Backlink::fragments`], and sources come ordered by
    /// permalink so a page compiles against the same value on every build.
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
    /// the feature is off, so nothing validates a page against a graph it never
    /// saw.
    pub fn digest(&self, page: &Page) -> Option<Hash> {
        match self {
            Self::Off => None,
            Self::On(_) => Some(Hash::of(&self.value(page))),
        }
    }
}

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
    /// Not a managed page link (external, fragment, or non-`.typ`); leave as
    /// authored.
    Passthrough,
    /// An internal `.typ` link resolved to the page it names.
    Resolved(Target),
    /// An internal `.typ` link whose target page does not exist.
    Broken,
}

/// The link-map entries a page's resolution consulted: for each source path
/// probed, the permalink it mapped to, or `None` when no page sat there.
///
/// A `None` carries as much weight as a `Some`: a link that fell through to the
/// base page because the site had no `.de` edition of it must rebuild when that
/// edition appears.
pub type LinkDeps = BTreeMap<PathBuf, Option<String>>;

/// A page's dependency on *which URLs the site serves*, keyed by the URL a link
/// named and holding whether a page sat there.
///
/// A `false` is the load-bearing half: a link to a page that does not exist yet
/// leaves no other trace on the linking page, so without the probe that page
/// can appear and the linker stays a cache hit.
pub type UrlDeps = BTreeMap<String, bool>;

/// Whether a link spelled as a URL reaches a page of this site, and the probe
/// that decided it.
pub struct Serving {
    /// The page it names, if the site serves one there.
    pub target: Option<Target>,
    pub probed: UrlDeps,
}

/// How one raw link resolved, and the map entries the outcome depended on.
pub struct Resolution {
    pub link: Link,
    pub probed: LinkDeps,
}

impl Resolution {
    fn passthrough() -> Self {
        Self {
            link: Link::Passthrough,
            probed: LinkDeps::new(),
        }
    }
}

/// Maps content source files to their resolved permalinks.
///
/// Links written against source paths (the typst-native way to cross-reference
/// pages) resolve to the target page's clean URL, so links survive permalink
/// changes.
#[derive(Debug)]
pub struct LinkMap {
    by_source: HashMap<PathBuf, String>,
    /// The extensions that make a link a source-path link rather than a URL or
    /// a file link, from [`Config::sources`](crate::config::Config::sources).
    sources: Vec<&'static str>,
    /// Every permalink this site serves, so a link written as a URL rather than
    /// as a source path can still be recognized as naming a page. Generated
    /// listings are in here even though they are not in `by_source`.
    urls: HashSet<String>,
    /// The typst project root: absolute link paths (`/posts/hello.typ`)
    /// resolve against it, mirroring typst's own path convention.
    root: PathBuf,
}

/// Hand-written: derived, `sources` would be empty, and a map that recognizes
/// no source path at all classifies every link as a URL.
impl Default for LinkMap {
    fn default() -> Self {
        Self {
            by_source: HashMap::new(),
            sources: crate::config::Config::default().sources(),
            urls: HashSet::new(),
            root: PathBuf::new(),
        }
    }
}

impl LinkMap {
    /// Index every page by the resolved path of its source file, the spelling
    /// [`LinkMap::candidates`] probes with. `root` is the typst project root
    /// absolute references resolve against.
    ///
    /// Generated listings are excluded: their source path is fabricated and no
    /// file sits there, so no author can write a source link against it.
    /// `sources` is the site's
    /// [`Config::sources`](crate::config::Config::sources), which decides what
    /// counts as a source path at all.
    pub fn new(pages: &[Page], root: &Path, sources: Vec<&'static str>) -> Self {
        let by_source = pages
            .iter()
            .filter(|p| p.authored())
            .map(|p| (crate::fs::resolved(&p.source), p.permalink.clone()))
            .collect();
        Self {
            by_source,
            sources,
            urls: pages.iter().map(|p| p.permalink.clone()).collect(),
            root: root.to_path_buf(),
        }
    }

    /// Every indexed page as `(source path, permalink)`.
    pub fn entries(&self) -> impl Iterator<Item = (&Path, &str)> {
        self.by_source
            .iter()
            .map(|(source, permalink)| (source.as_path(), permalink.as_str()))
    }

    /// The page a raw link already spelled as a URL names, if this site serves
    /// one there, and the probe that decided it.
    pub fn served(&self, raw: &str) -> Serving {
        let target = Target::from(raw);
        let serves = self.urls.contains(target.page());
        Serving {
            probed: UrlDeps::from([(target.page().to_owned(), serves)]),
            target: serves.then_some(target),
        }
    }

    /// Every URL this site serves a page at.
    pub fn urls(&self) -> &HashSet<String> {
        &self.urls
    }

    /// Classify a raw link written in `from`'s body: passthrough, resolved to a
    /// permalink, or a broken internal `.typ` reference, together with the map
    /// entries that decided it.
    ///
    /// `lang` is the linking page's language on a multilingual site, `None`
    /// otherwise: a translated page writes the same `#link("b.typ")` as its
    /// original and means its own edition of `b`.
    pub fn classify(&self, raw: &str, from: &Path, lang: Option<&str>) -> Resolution {
        if Self::is_external(raw) {
            return Resolution::passthrough();
        }
        let split = super::Tail::of(raw);
        let Some(extension) = Path::new(split.path).extension() else {
            return Resolution::passthrough();
        };
        if !self.sources.iter().any(|source| extension == *source) {
            return Resolution::passthrough();
        }
        let target = split.path.strip_prefix('/').map_or_else(
            || {
                from.parent()
                    .unwrap_or_else(|| Path::new("."))
                    .join(split.path)
            },
            |rooted| self.root.join(rooted),
        );
        let mut probed = LinkDeps::new();
        let resolved = Self::candidates(&target, lang).find_map(|candidate| {
            let permalink = self.by_source.get(&candidate).cloned();
            probed.insert(candidate, permalink.clone());
            permalink
        });
        let link = resolved.map_or(Link::Broken, |permalink| {
            Link::Resolved(Target::new(&permalink, raw))
        });
        Resolution { link, probed }
    }

    /// The source paths a link to `target` probes, in order: the reader's own
    /// language edition first, then the target as written.
    ///
    /// Spelled by [`crate::fs::resolved`] and never [`crate::fs::canonical`]:
    /// a probe resolving to nothing is recorded as a dependency, so it has to
    /// spell the same for a target that is not there as for one that is.
    fn candidates(target: &Path, lang: Option<&str>) -> impl Iterator<Item = PathBuf> {
        lang.and_then(|lang| Self::edition(target, lang))
            .into_iter()
            .chain(std::iter::once(target.to_path_buf()))
            .map(crate::fs::resolved)
    }

    /// The `{stem}.{lang}.{ext}` sibling of `target`: the reader's own edition
    /// of the page a link points at.
    ///
    /// The extension is the target's own, not `typ`: a link to a markdown page
    /// means its markdown edition.
    fn edition(target: &Path, lang: &str) -> Option<PathBuf> {
        let stem = target.file_stem()?.to_str()?;
        let extension = target.extension()?.to_str()?;
        Some(target.with_file_name(format!("{stem}.{lang}.{extension}")))
    }

    /// Whether a link points outside the site (a scheme, or protocol-relative)
    /// and must be left as authored.
    ///
    /// A bare `#fragment` is *not* external: it names a section of the page it
    /// was written on, and is recorded by the render pass, which is the only
    /// place that knows which page that is.
    fn is_external(raw: &str) -> bool {
        raw.starts_with("//") || Self::scheme(raw)
    }

    /// Whether `raw` opens with an RFC 3986 URL scheme (`https:`, `mailto:`).
    ///
    /// Tested on the head alone, what precedes the first `/`, `?` or `#`:
    /// matched anywhere, `b.typ?redirect=https://x` reads as a link off the
    /// site and is published as the literal source path.
    fn scheme(raw: &str) -> bool {
        let head = raw.split(['/', '?', '#']).next().unwrap_or(raw);
        head.split_once(':')
            .is_some_and(|(scheme, _)| Scheme::valid(scheme))
    }
}

#[cfg(test)]
mod tests {
    use super::{Backlinks, LinkMap, Outbound, Source, Target, UrlDeps};
    use crate::content::{Data, Frontmatter, Page, PageId};
    use std::path::PathBuf;

    #[test]
    fn a_target_names_one_page_and_at_most_one_section() {
        for (raw, page, fragment) in [
            ("/b/", "/b/", None),
            ("/b/#install", "/b/", Some("install")),
            ("/b/?x=1", "/b/", None),
            ("/b/#", "/b/", None),
            // A `?` after the `#` is part of the fragment, and a `?` before it
            // does not hide the fragment: RFC 3986 orders query then fragment.
            ("/b/#install?x=1", "/b/", Some("install?x=1")),
            ("/b/?x=1#install", "/b/", Some("install")),
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

    /// A target is stored as the URL it names and nothing else, so a manifest
    /// round-trips without a [`crate::graph::Renderer::SCHEMA`] bump.
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
        assert!(backlinks.of(&source).is_empty());
    }

    fn urls(outbound: &Outbound) -> Vec<String> {
        outbound.targets().map(Target::to_string).collect()
    }

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
        }
    }

    #[test]
    fn external_links_are_recognized() {
        for raw in ["https://example.com", "http://x", "//cdn", "mailto:a@b.c"] {
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
    fn a_scheme_is_only_read_at_the_head_of_a_link() {
        for raw in [
            "b.typ?redirect=https://x",
            "b.typ#see-https://x",
            "../a/b.typ?to=mailto:a@b.c",
        ] {
            assert!(!LinkMap::is_external(raw), "{raw} should be local");
        }
        for raw in ["data:text/plain,x", "git+ssh://host/repo", "HTTPS://x"] {
            assert!(LinkMap::is_external(raw), "{raw} should be external");
        }
    }

    #[test]
    fn a_bare_fragment_is_not_a_link_off_the_site() {
        for raw in ["#install", "#", "#install?x=1"] {
            assert!(!LinkMap::is_external(raw), "{raw} should be local");
        }
        let map = LinkMap::default();
        let resolution = map.classify("#install", std::path::Path::new("a.typ"), None);
        assert_eq!(resolution.link, super::Link::Passthrough);
        assert!(resolution.probed.is_empty());
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
    fn a_link_naming_a_markdown_page_resolves_like_any_other() {
        use super::Link;
        let mut markdown = page("Notes", "/notes/");
        markdown.source = PathBuf::from("content/notes.md");
        let pages = [page("A", "/a/"), markdown];
        let map = LinkMap::new(&pages, std::path::Path::new("."), vec!["typ", "md"]);
        let from = std::path::Path::new("content/a.typ");

        assert_eq!(
            map.classify("notes.md", from, None).link,
            Link::Resolved(Target::from("/notes/")),
        );
        assert_eq!(map.classify("gone.md", from, None).link, Link::Broken);
    }

    #[test]
    fn a_markdown_link_passes_through_where_markdown_is_not_a_source() {
        use super::Link;
        let pages = [page("A", "/a/")];
        let map = LinkMap::new(&pages, std::path::Path::new("."), vec!["typ"]);
        let from = std::path::Path::new("content/a.typ");

        assert_eq!(map.classify("notes.md", from, None).link, Link::Passthrough);
        assert!(map.classify("notes.md", from, None).probed.is_empty());
    }

    /// Built with explicit sources rather than [`LinkMap::default`], whose set
    /// is `typ` alone with the `markdown` feature off, leaving the test
    /// asserting nothing.
    #[test]
    fn the_edition_probed_keeps_the_targets_own_extension() {
        let map = LinkMap::new(&[], std::path::Path::new("."), vec!["typ", "md"]);
        let from = std::path::Path::new("content/a.md");

        let probed = map.classify("b.md", from, Some("de")).probed;

        let names: Vec<String> = probed
            .keys()
            .map(|path| path.display().to_string())
            .collect();
        assert!(
            names.iter().any(|n| n.ends_with("b.de.md")),
            "the german edition of a markdown page: {names:?}"
        );
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
    fn a_link_spelled_as_a_url_still_names_a_page() {
        let pages = [page("A", "/a/"), page("B", "/b/")];
        let map = LinkMap::new(&pages, std::path::Path::new("."), vec!["typ", "md"]);

        let page = |raw| map.served(raw).target.map(|t| t.page().to_owned());
        assert_eq!(page("/b/").as_deref(), Some("/b/"));
        assert_eq!(page("/b/#install").as_deref(), Some("/b/"));
        assert_eq!(page("/nowhere/"), None);
        assert_eq!(page("https://example.com/b/"), None);
    }

    #[test]
    fn a_url_link_records_its_probe_whether_or_not_it_named_a_page() {
        let pages = [page("A", "/a/"), page("B", "/b/")];
        let map = LinkMap::new(&pages, std::path::Path::new("."), vec!["typ", "md"]);

        assert_eq!(
            map.served("/b/#install").probed,
            UrlDeps::from([("/b/".to_owned(), true)]),
            "the section is not part of what was probed"
        );
        assert_eq!(
            map.served("/nowhere/").probed,
            UrlDeps::from([("/nowhere/".to_owned(), false)])
        );
    }

    #[test]
    fn a_scan_finds_the_links_a_source_writes_out() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        std::fs::create_dir_all(root.join("content")).unwrap();
        for name in ["a.typ", "b.typ"] {
            std::fs::write(root.join("content").join(name), "").unwrap();
        }
        let pages = [page("A", "/a/"), page("B", "/b/")];
        let map = LinkMap::new(&pages, root, vec!["typ", "md"]);
        let text = r#"#link("b.typ")[to b] #link("a.typ")[self] #image("photo.png") "b.typ""#;
        let source = Source::detached(text);

        let scanned = Outbound::scanned(&source, &pages[0].source, &pages[0].permalink, &map, None);

        assert_eq!(urls(&scanned), ["/b/"]);
    }

    #[test]
    fn a_multilingual_link_probes_the_edition_before_the_target() {
        let map = LinkMap::default();
        let from = std::path::Path::new("a.typ");

        let probed = map.classify("b.typ", from, Some("de")).probed;

        let names: Vec<_> = probed
            .keys()
            .filter_map(|p| p.file_name()?.to_str())
            .collect();
        assert_eq!(names, ["b.de.typ", "b.typ"], "{probed:?}");
    }
}
