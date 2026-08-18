//! `html { }`: what the rendered markup carries.

pub mod anchors;
pub mod highlight;
pub mod math;
pub mod meta;
pub mod region;

use crate::config::dispatch::Kind::{Block as Nested, Flag, Texts};
use crate::config::dispatch::{Block, Section};
use crate::config::node::NodeExt;
use crate::config::{AnchorConfig, HighlightConfig, MathConfig, MetaConfig, RegionConfig};
use crate::error::ConfigError;

#[derive(Debug, Clone, Hash)]
pub struct HtmlConfig {
    pub pretty: bool,
    /// Inline local assets (`/assets/..` refs) as `data:` URIs.
    pub embed: bool,
    /// SEO and social meta tags (description, OpenGraph, Twitter, canonical) in
    /// each page's `<head>`, from frontmatter and config.
    pub meta: MetaConfig,
    /// Deep-linkable headings: a slug `id` where one is missing, and the link
    /// back to it.
    pub anchors: AnchorConfig,
    /// Which part of a rendered page is its prose, read by both the search
    /// index and a full-content feed.
    pub region: RegionConfig,
    /// Class a code block's tokens instead of colouring them inline.
    pub highlight: HighlightConfig,
    /// Where the CSS that typst's MathML output depends on lives.
    pub math: MathConfig,
    /// Emit a schema.org JSON-LD island in each page's `<head>`.
    pub jsonld: bool,
    /// Where a page's footnotes are moved to.
    pub footnotes: Footnotes,
    /// Stamp every element with the `file:line:column` it was authored at, as
    /// `data-typst`. A config field rather than a `serve`-only flag, because
    /// `serve` settings are excluded from the cache fingerprint and a
    /// mode-derived stamp would leave a `build` reusing a served page's markup.
    pub spans: bool,
}

/// The elements a page's footnote list is moved into, most specific first; the
/// first one a page has wins, and an empty list keeps Typst's own placement.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Footnotes(Vec<String>);
impl Default for Footnotes {
    fn default() -> Self {
        Self(vec!["article".to_owned(), "main".to_owned()])
    }
}

impl Footnotes {
    pub fn targets(&self) -> &[String] {
        &self.0
    }

    /// Whether the notes stay where Typst put them.
    pub fn disabled(&self) -> bool {
        self.0.is_empty()
    }
}

impl From<Vec<String>> for Footnotes {
    fn from(names: Vec<String>) -> Self {
        Self(names)
    }
}

impl Default for HtmlConfig {
    fn default() -> Self {
        Self {
            pretty: true,
            embed: false,
            meta: MetaConfig::default(),
            anchors: AnchorConfig::default(),
            region: RegionConfig::default(),
            highlight: HighlightConfig::default(),
            math: MathConfig::default(),
            jsonld: false,
            footnotes: Footnotes::default(),
            spans: false,
        }
    }
}

impl Section for HtmlConfig {
    const RULES: Block<Self> = Block(&[
        (
            "pretty",
            Flag,
            "Indent the emitted HTML.",
            |c| c.pretty.into(),
            |c, n, t| {
                c.pretty = n.boolean(t, 0)?;
                Ok(())
            },
        ),
        (
            "embed",
            Flag,
            "Inline processed assets into the page as `data:` URIs.",
            |c| c.embed.into(),
            |c, n, t| {
                c.embed = n.boolean(t, 0)?;
                Ok(())
            },
        ),
        (
            "meta",
            Nested(MetaConfig::rows),
            "The `<meta>` description, Open Graph and Twitter tags. On by default; `#false` turns them off.",
            |c| c.meta.values(),
            |c, n, t| c.meta.fill(n, t),
        ),
        (
            "anchors",
            Nested(AnchorConfig::rows),
            "Give every heading an `id`, and optionally a link back to it. On by default; `#false` turns it off.",
            |c| c.anchors.values(),
            |c, n, t| c.anchors.fill(n, t),
        ),
        (
            "region",
            Nested(RegionConfig::rows),
            "Which part of a rendered page is its prose.",
            |c| c.region.values(),
            |c, n, t| c.region.fill(n, t),
        ),
        (
            "math",
            Nested(MathConfig::rows),
            "Where the CSS that typst's MathML output depends on lives.",
            |c| c.math.values(),
            |c, n, t| c.math.fill(n, t),
        ),
        (
            "jsonld",
            Flag,
            "Emit JSON-LD structured data for each page.",
            |c| c.jsonld.into(),
            |c, n, t| {
                c.jsonld = n.boolean(t, 0)?;
                Ok(())
            },
        ),
        (
            "spans",
            Flag,
            "Stamp each element with the source span it came from, so `serve` can open it.",
            |c| c.spans.into(),
            |c, n, t| {
                c.spans = n.boolean(t, 0)?;
                Ok(())
            },
        ),
        (
            "footnotes",
            Texts,
            "The elements a page's footnotes belong inside, most specific first.",
            |c| c.footnotes.targets().iter().cloned().collect(),
            |c, n, t| {
                let span = NodeExt::span(n);
                let names = n.words(t)?;
                for name in &names {
                    typst_html::HtmlTag::intern(name)
                        .map_err(|why| ConfigError::not_an_element(t, name, &why, span))?;
                }
                c.footnotes = names.into();
                Ok(())
            },
        ),
        (
            "highlight",
            Nested(HighlightConfig::rows),
            "Class a code block's tokens (`sx-keyword`, ..) instead of colouring them inline.",
            |c| c.highlight.values(),
            |c, n, t| c.highlight.fill(n, t),
        ),
    ]);
}
