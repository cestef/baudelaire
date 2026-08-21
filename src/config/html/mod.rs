//! `html { }`: what the rendered markup carries.

pub mod anchors;
pub mod highlight;
pub mod math;
pub mod meta;
pub mod region;

use dispatch_derive::Table;

use crate::config::dispatch::Kind::Texts;
use crate::config::dispatch::{Block, Section};
use crate::config::node::NodeExt;
use crate::config::vocab::rule;
use crate::config::{AnchorConfig, HighlightConfig, MathConfig, MetaConfig, RegionConfig};
use crate::error::ConfigError;

#[derive(Debug, Clone, Hash, Table)]
pub struct HtmlConfig {
    /// Indent the emitted HTML.
    #[key(flag)]
    pub pretty: bool,

    /// Inline processed assets into the page as `data:` URIs.
    #[key(flag)]
    pub embed: bool,

    /// The `<meta>` description, Open Graph and Twitter tags. On by default; `#false` turns them off.
    #[key(nested(MetaConfig))]
    pub meta: MetaConfig,

    /// Give every heading an `id`, and optionally a link back to it. On by default; `#false` turns it off.
    #[key(nested(AnchorConfig))]
    pub anchors: AnchorConfig,

    /// Which part of a rendered page is its prose.
    ///
    /// Read by both the search index and a full-content feed.
    #[key(nested(RegionConfig))]
    pub region: RegionConfig,

    /// Where the CSS that typst's MathML output depends on lives.
    #[key(nested(MathConfig))]
    pub math: MathConfig,

    /// Emit JSON-LD structured data for each page.
    #[key(flag)]
    pub jsonld: bool,

    /// Stamp each element with the source span it came from, so `serve` can open it.
    ///
    /// A config field rather than a `serve`-only flag, because `serve` settings
    /// are excluded from the cache fingerprint and a mode-derived stamp would
    /// leave a `build` reusing a served page's markup.
    #[key(flag)]
    pub spans: bool,

    /// The elements a page's footnotes belong inside, most specific first.
    #[key(custom(
        Texts,
        |c: &Self| c.footnotes.targets().iter().cloned().collect(),
        |c: &mut Self, n: &kdl::KdlNode, t: &str| {
            let span = NodeExt::span(n);
            let names = n.words(t)?;
            for name in &names {
                typst_html::HtmlTag::intern(name)
                    .map_err(|why| ConfigError::not_an_element(t, name, &why, span))?;
            }
            c.footnotes = names.into();
            Ok(())
        },
    ))]
    pub footnotes: Footnotes,

    /// Class a code block's tokens (`sx-keyword`, ..) instead of colouring them inline.
    #[key(nested(HighlightConfig))]
    pub highlight: HighlightConfig,
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
            math: MathConfig::default(),
            jsonld: false,
            spans: false,
            footnotes: Footnotes::default(),
            highlight: HighlightConfig::default(),
        }
    }
}
