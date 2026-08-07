//! `html { }`: what the rendered markup carries.

pub mod anchors;
pub mod highlight;
pub mod region;

use crate::config::dispatch::Kind::{Block as Nested, Flag, Table, Texts};
use crate::config::dispatch::{Block, Section};
use crate::config::node::NodeExt;
use crate::config::{AnchorConfig, HighlightConfig, RegionConfig};
use crate::error::ConfigError;

/// HTML output options.
#[derive(Debug, Clone, Hash)]
pub struct HtmlConfig {
    /// Pretty-print HTML.
    pub pretty: bool,
    /// Inline local assets (`/assets/..` refs) as `data:` URIs.
    pub embed: bool,
    /// Inject SEO + social meta tags (description, OpenGraph, Twitter, canonical)
    /// into each page's `<head>` from frontmatter and config.
    pub meta: bool,
    /// Deep-linkable headings: a slug `id` where one is missing, and the link
    /// back to it.
    pub anchors: AnchorConfig,
    /// Which part of a rendered page is its prose: read by the search index and
    /// by a full-content feed, so both mean the same thing by it.
    pub region: RegionConfig,
    /// Rewrite syntax-highlight colours as CSS classes.
    pub highlight: HighlightConfig,
    /// Emit a schema.org JSON-LD island in each page's `<head>`.
    ///
    /// Opt-in, unlike the meta tags beside it: those restate facts the page
    /// already states, while structured data is a claim made *to* a search
    /// engine about what the page is, and that is the author's claim to make.
    pub jsonld: bool,
    /// Where a page's footnotes are moved to.
    pub footnotes: Footnotes,
    /// Stamp every element with the `file:line:column` it was authored at, as
    /// `data-typst`. What a source-mapped preview reads to jump from a rendered
    /// element back to the Typst that produced it.
    ///
    /// Opt-in, and off in a published build: the attributes are for the author,
    /// not the reader. `serve --spans` turns them on for a preview session.
    /// Deliberately a config field rather than a `serve`-only flag: `serve`
    /// settings are excluded from the cache fingerprint, so a mode-derived
    /// stamp would leave a `build` reusing a served page's markup, attributes
    /// and all.
    pub spans: bool,
}

/// The elements a page's footnote list is moved into, most specific first.
///
/// Typst appends the list to the end of the document, which is right for a page
/// with no template and wrong for one with a layout: everything the layout emits
/// is already in the body, so the notes land after the site footer, outside the
/// element that sets the content width.
///
/// This is a list rather than one name because a site's layouts rarely agree: a
/// post wraps its body in `<article>`, a generated index has only `<main>`, and
/// a bespoke page may have neither. Each name is tried in order and the first
/// element found wins, so `footnotes "article" "main"` covers all three without
/// a rule per template. An empty list moves nothing, which is how a site keeps
/// Typst's own placement.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Footnotes(Vec<String>);
impl Default for Footnotes {
    /// An article, else the main region: the two elements a layout is most
    /// likely to have, in the order that puts the notes closest to the text
    /// they annotate.
    fn default() -> Self {
        Self(vec!["article".to_owned(), "main".to_owned()])
    }
}

impl Footnotes {
    /// The element names to try, in order.
    pub fn targets(&self) -> &[String] {
        &self.0
    }

    /// Whether the notes stay where Typst put them.
    pub fn disabled(&self) -> bool {
        self.0.is_empty()
    }
}

/// Built from the configured names, which the parser has already checked are
/// element names the DOM can hold.
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
            meta: true,
            anchors: AnchorConfig::default(),
            region: RegionConfig::default(),
            highlight: HighlightConfig::default(),
            // opt-in: structured data is a claim about the page, not a restating
            // of what it already says.
            jsonld: false,
            footnotes: Footnotes::default(),
            // opt-in: source spans are scaffolding for whoever is writing the
            // page, and every reader of the published one would pay for them.
            spans: false,
        }
    }
}

/// The `html { .. }` section: post-processing of typst's HTML output.
impl Section for HtmlConfig {
    const RULES: Block<Self> = Block(&[
        ("pretty", Flag, "Indent the emitted HTML.", |c, n, t| {
            c.pretty = n.boolean(t, 0)?;
            Ok(())
        }),
        (
            "embed",
            Flag,
            "Inline processed assets into the page as `data:` URIs.",
            |c, n, t| {
                c.embed = n.boolean(t, 0)?;
                Ok(())
            },
        ),
        (
            "meta",
            Flag,
            "Emit the `<meta>` description, Open Graph and Twitter tags.",
            |c, n, t| {
                c.meta = n.boolean(t, 0)?;
                Ok(())
            },
        ),
        (
            "anchors",
            Nested(AnchorConfig::rows),
            "Give every heading an `id`, and optionally a link back to it. On by default; `#false` turns it off.",
            |c, n, t| c.anchors.fill(n, t),
        ),
        (
            "region",
            Nested(RegionConfig::rows),
            "Which part of a rendered page is its prose.",
            |c, n, t| c.region.fill(n, t),
        ),
        (
            "jsonld",
            Flag,
            "Emit JSON-LD structured data for each page.",
            |c, n, t| {
                c.jsonld = n.boolean(t, 0)?;
                Ok(())
            },
        ),
        (
            "spans",
            Flag,
            "Stamp each element with the source span it came from, so `serve` can open it.",
            |c, n, t| {
                c.spans = n.boolean(t, 0)?;
                Ok(())
            },
        ),
        // A list of element names, tried in order. Each is checked here, where
        // the span points at the word the author wrote: an unwritable name would
        // otherwise fail silently at render, as an element the page never has.
        (
            "footnotes",
            Texts,
            "The elements a page's footnotes belong inside, most specific first.",
            |c, n, t| {
                let span = NodeExt::span(n);
                let names = n.words(t)?;
                for name in &names {
                    // The DOM's own judgement of what can be an element, rather
                    // than a second opinion here; why it is judged at this span
                    // is on `NotAnElement`.
                    typst_html::HtmlTag::intern(name)
                        .map_err(|why| ConfigError::not_an_element(t, name, &why, span))?;
                }
                c.footnotes = names.into();
                Ok(())
            },
        ),
        (
            "highlight",
            Table,
            "Rewrite syntax-highlight colours to classes. Bare, it uses hex classes; a block names the scopes.",
            |c, n, t| {
                // The flag `Section::shorthand` would dispatch, spelled out
                // here because the block is a free-form scope table and not a
                // key set: a bare `highlight` turns it on, and `highlight
                // #false` turns it off again. Off has to be sayable, because
                // every shipped theme turns it on and a `theme.kdl` is a floor
                // the site is meant to be able to override.
                c.highlight.enabled = n.boolean(t, 0)?;
                // A bare `highlight` rewrites every colour to its hex class; a block
                // names the scopes the theme paints, so the classes read as meaning
                // rather than as colours.
                if n.children().is_some() {
                    c.highlight.scopes = n.pairs(t)?;
                }
                Ok(())
            },
        ),
    ]);
}
