//! Fills in the tables of contents that `@baudelaire/toc`'s `toc()` marked.
//!
//! A page's heading set only exists once typst has produced the DOM, which is
//! why this is a pass and not a module binding.

use std::sync::LazyLock;

use typst::syntax::Span;
use typst_html::{HtmlAttr, HtmlDocument, HtmlElement, HtmlNode, HtmlTag, attr, tag};

use crate::config::{Config, RegionConfig};
use crate::render::prose::Prose;
use crate::world::module::Toc as Marked;

use super::{Anchors, AttrsExt, Cx, DocumentExt, ElementExt, Exempt, Transform};

static MARKER: LazyLock<HtmlAttr> =
    LazyLock::new(|| HtmlAttr::intern(Marked::MARKER).expect("marker is a valid attribute name"));

static LIST: LazyLock<HtmlAttr> =
    LazyLock::new(|| HtmlAttr::intern(Marked::LIST).expect("marker is a valid attribute name"));

/// The heading levels one table of contents lists, both ends included.
struct Levels {
    from: u8,
    to: u8,
}

impl Levels {
    /// `"<from>-<to>"`, as `toc()` wrote it.
    fn parse(spec: &str) -> Option<Self> {
        let (from, to) = spec.split_once('-')?;
        Some(Self {
            from: from.parse().ok()?,
            to: to.parse().ok()?,
        })
    }

    fn covers(&self, level: u8) -> bool {
        (self.from..=self.to).contains(&level)
    }
}

/// One heading a table of contents links to.
struct Entry {
    level: u8,
    id: String,
    text: String,
}

/// The [`Transform`] that builds a table of contents into each marked element.
pub(super) struct Toc;

impl Transform for Toc {
    fn name(&self) -> &'static str {
        Self::NAME
    }

    fn after(&self) -> &'static [&'static str] {
        &[Exempt::NAME, Anchors::NAME]
    }

    /// Always on: importing `toc()` is itself the opt-in.
    fn enabled(&self, _config: &Config) -> bool {
        true
    }

    fn apply(&self, doc: &mut HtmlDocument, cx: &mut Cx<'_>) {
        if !Self::marked(doc) {
            return;
        }
        let entries = Self::headings(doc, &cx.config.html.region);
        doc.walk(|element| {
            let Some(spec) = element.attrs.get(*MARKER).cloned() else {
                return;
            };
            let list = Self::list(element);
            element.attrs.remove(*MARKER);
            element.attrs.remove(*LIST);
            let Some(levels) = Levels::parse(&spec) else {
                return;
            };
            let items: Vec<&Entry> = entries
                .iter()
                .filter(|entry| levels.covers(entry.level))
                .collect();
            element.children = Self::build(&items, list);
        });
    }
}

impl Toc {
    /// How [`Transform::after`] names this pass.
    pub(super) const NAME: &'static str = "toc";

    /// Whether the page marked anything at all, checked read-only so a page
    /// with no table of contents never pays for [`DocumentExt::walk`], which
    /// clones each shared child list.
    fn marked(doc: &HtmlDocument) -> bool {
        let mut found = false;
        doc.visit(|element| found |= element.attrs.get(*MARKER).is_some());
        found
    }

    /// The list element to build, `<ol>` when `toc(ordered: true)` said so.
    fn list(element: &HtmlElement) -> HtmlTag {
        match element.attrs.get(*LIST) {
            Some(name) if name.as_str() == tag::ol.resolve().as_str() => tag::ol,
            _ => tag::ul,
        }
    }

    /// The page's headings in document order, taken from the region
    /// `html { region }` names so a layout's chrome is never listed.
    ///
    /// A heading with no `id` is left out: nothing can link to it.
    fn headings(doc: &HtmlDocument, region: &RegionConfig) -> Vec<Entry> {
        let prose = Prose::from(region);
        let mut out = Vec::new();
        prose.visit(prose.region(doc.root()), &mut |element| {
            let Some(level) = element.heading() else {
                return;
            };
            let Some(id) = element.attrs.get(attr::id) else {
                return;
            };
            out.push(Entry {
                level,
                id: id.to_string(),
                text: element.text(),
            });
        });
        out
    }

    /// The whole nested list, or nothing at all when no heading is in range.
    fn build(items: &[&Entry], list: HtmlTag) -> typst::ecow::EcoVec<HtmlNode> {
        let Some(shallowest) = items.iter().map(|entry| entry.level).min() else {
            return typst::ecow::EcoVec::new();
        };
        let mut at = 0;
        std::iter::once(HtmlNode::from(Self::nest(items, &mut at, shallowest, list))).collect()
    }

    /// The list of the entries at `depth`, from `*at` onwards, each carrying
    /// whatever runs deeper than it.
    fn nest(items: &[&Entry], at: &mut usize, depth: u8, list: HtmlTag) -> HtmlElement {
        let mut out = HtmlElement::new(list);
        while let Some(item) = items.get(*at) {
            if item.level < depth {
                break;
            }
            if item.level > depth {
                let deeper = Self::nest(items, at, item.level, list);
                Self::adopt(&mut out, deeper);
                continue;
            }
            *at += 1;
            out.children.push(Self::entry(item).into());
        }
        out
    }

    /// Put a deeper list under the entry above it, or in an entry of its own
    /// where the page skipped a level and there is none.
    fn adopt(out: &mut HtmlElement, deeper: HtmlElement) {
        if let Some(HtmlNode::Element(last)) = out.children.make_mut().last_mut() {
            last.children.push(deeper.into());
            return;
        }
        out.children
            .push(Self::li(std::iter::once(HtmlNode::from(deeper)).collect()).into());
    }

    /// One entry: a link to the heading, by the text the heading reads as.
    fn entry(item: &Entry) -> HtmlElement {
        let link = HtmlElement::new(tag::a)
            .with_attr(attr::href, format!("#{}", item.id))
            .with_children(
                std::iter::once(HtmlNode::Text(item.text.as_str().into(), Span::detached()))
                    .collect(),
            );
        Self::li(std::iter::once(HtmlNode::from(link)).collect())
    }

    fn li(children: typst::ecow::EcoVec<HtmlNode>) -> HtmlElement {
        HtmlElement::new(tag::li).with_children(children)
    }
}

#[cfg(test)]
mod tests {
    use super::{Entry, Levels, Toc};
    use typst_html::{HtmlElement, HtmlNode, tag};

    fn entries(levels: &[u8]) -> Vec<Entry> {
        levels
            .iter()
            .enumerate()
            .map(|(i, &level)| Entry {
                level,
                id: format!("h{i}"),
                text: format!("H{i}"),
            })
            .collect()
    }

    /// The shape of a built list, as `tag[children]`, so a nesting bug reads as
    /// a diff rather than as a wall of markup.
    fn shape(element: &HtmlElement) -> String {
        let inner: Vec<String> = element
            .children
            .iter()
            .filter_map(|node| match node {
                HtmlNode::Element(child) => Some(shape(child)),
                HtmlNode::Text(text, _) => Some(text.to_string()),
                _ => None,
            })
            .collect();
        format!("{}[{}]", element.tag.resolve(), inner.join(""))
    }

    fn built(levels: &[u8]) -> String {
        let entries = entries(levels);
        let items: Vec<&Entry> = entries.iter().collect();
        let nodes = Toc::build(&items, tag::ul);
        nodes
            .iter()
            .filter_map(|node| match node {
                HtmlNode::Element(element) => Some(shape(element)),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn a_flat_run_of_headings_is_one_list() {
        assert_eq!(
            built(&[2, 2]),
            "ul[li[a[H0]]li[a[H1]]]",
            "two peers should be siblings"
        );
    }

    #[test]
    fn a_deeper_heading_nests_under_the_one_above_it() {
        assert_eq!(
            built(&[2, 3, 3, 2]),
            "ul[li[a[H0]ul[li[a[H1]]li[a[H2]]]]li[a[H3]]]"
        );
    }

    /// `== A` then `==== B`: nothing at level 3 to hang the deeper list off.
    #[test]
    fn a_skipped_level_still_nests() {
        assert_eq!(built(&[2, 4, 2]), "ul[li[a[H0]ul[li[a[H1]]]]li[a[H2]]]");
    }

    /// A page whose first heading is deeper than a later one: the list starts
    /// at the shallowest level there is, not at the first one seen.
    #[test]
    fn the_list_starts_at_the_shallowest_level_present() {
        assert_eq!(built(&[3, 2]), "ul[li[ul[li[a[H0]]]]li[a[H1]]]");
    }

    #[test]
    fn no_heading_in_range_builds_nothing() {
        assert!(built(&[]).is_empty());
    }

    #[test]
    fn levels_read_the_range_toc_wrote() {
        let levels = Levels::parse("2-4").unwrap();
        assert!(!levels.covers(1));
        assert!(levels.covers(2));
        assert!(levels.covers(4));
        assert!(!levels.covers(5));
        assert!(Levels::parse("2").is_none());
        assert!(Levels::parse("a-b").is_none());
    }
}
