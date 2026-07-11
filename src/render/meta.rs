//! Injects SEO and social meta tags into each page's `<head>`.
//!
//! When `html { meta true }` is set (the default), this appends a description,
//! OpenGraph, Twitter Card, and canonical `<link>` to every page, derived from
//! its frontmatter (`description`/`summary`, `image`, `author`, tags, date) and
//! the site config (`url`, `site`, `author`, `lang`). URL-absolute tags
//! (`og:url`, canonical) are emitted only when a base `url` is configured.
//!
//! typst-html owns the document `<head>` (templates can only set the title), so
//! these tags cannot be authored in a layout — appending them to the parsed DOM
//! here is the single place they can be added for every page at once.

use typst_html::{HtmlAttr, HtmlDocument, HtmlElement, HtmlNode, attr, tag};

use crate::config::Config;
use crate::content::Page;

use super::AssetMap;
use super::transform::{Cx, Transform};

/// The [`Transform`] that appends meta tags to `<head>`.
pub(super) struct Meta;

impl Transform for Meta {
    fn enabled(&self, config: &Config) -> bool {
        config.html.meta
    }

    fn apply(&self, doc: &mut HtmlDocument, cx: &mut Cx<'_>) {
        let tags = Card { config: cx.config, page: cx.page, assets: cx.assets }.tags();
        if tags.is_empty() {
            return;
        }
        if let Some(head) = Self::head(doc.root_mut()) {
            for node in tags {
                head.children.push(node);
            }
        }
    }
}

impl Meta {
    /// The document `<head>`, a direct child of the root `<html>` element.
    fn head(root: &mut HtmlElement) -> Option<&mut HtmlElement> {
        root.children.make_mut().iter_mut().find_map(|node| match node {
            HtmlNode::Element(el) if el.tag == tag::head => Some(el),
            _ => None,
        })
    }
}

/// Builds the meta tags for one page from its frontmatter and the site config.
struct Card<'a> {
    config: &'a Config,
    page: &'a Page,
    /// Processed-asset URL map, so a social image is named at its fingerprinted
    /// URL before it is absolutized (the fingerprint transform runs later and
    /// cannot resolve an already-absolute `content` value).
    assets: &'a AssetMap,
}

impl Card<'_> {
    fn tags(&self) -> Vec<HtmlNode> {
        let fm = &self.page.frontmatter;
        let title = fm.title.clone().unwrap_or_default();
        let description = fm.text("description").or_else(|| fm.text("summary"));
        let image = fm.text("image").map(|src| self.absolute(&src));
        let canonical = self.url();
        // A dated page is an article; everything else is a plain website page.
        let kind = if fm.date.is_some() { "article" } else { "website" };

        let mut tags = Vec::new();
        if let Some(description) = &description {
            tags.push(Self::named("description", description));
        }
        if let Some(author) = fm.text("author").or_else(|| self.config.author.clone()) {
            tags.push(Self::named("author", &author));
        }

        // OpenGraph.
        tags.push(Self::property("og:type", kind));
        if !title.is_empty() {
            tags.push(Self::property("og:title", &title));
        }
        if let Some(description) = &description {
            tags.push(Self::property("og:description", description));
        }
        if let Some(url) = &canonical {
            tags.push(Self::property("og:url", url));
        }
        if let Some(site) = &self.config.site {
            tags.push(Self::property("og:site_name", site));
        }
        tags.push(Self::property("og:locale", &self.config.lang));
        if let Some(image) = &image {
            tags.push(Self::property("og:image", image));
        }

        // Twitter Card.
        tags.push(Self::named(
            "twitter:card",
            if image.is_some() { "summary_large_image" } else { "summary" },
        ));
        if !title.is_empty() {
            tags.push(Self::named("twitter:title", &title));
        }
        if let Some(description) = &description {
            tags.push(Self::named("twitter:description", description));
        }
        if let Some(image) = &image {
            tags.push(Self::named("twitter:image", image));
        }

        if let Some(url) = &canonical {
            tags.push(Self::canonical(url));
        }
        tags
    }

    /// The page's canonical absolute URL, if a base `url` is configured.
    fn url(&self) -> Option<String> {
        let base = self.config.url.as_deref()?;
        Some(format!("{}{}", base.trim_end_matches('/'), self.page.permalink))
    }

    /// Resolve a root-relative asset reference to its fingerprinted URL, then
    /// make it absolute against the site `url`. An already-absolute (`http`)
    /// value, or one with no base URL, is left as authored (bar fingerprinting).
    fn absolute(&self, src: &str) -> String {
        let src = self.assets.resolve(src).unwrap_or_else(|| src.to_owned());
        match self.config.url.as_deref() {
            Some(base) if src.starts_with('/') => format!("{}{src}", base.trim_end_matches('/')),
            _ => src,
        }
    }

    /// A `<meta name="…" content="…">` tag.
    fn named(name: &str, content: &str) -> HtmlNode {
        Self::meta(attr::name, name, content)
    }

    /// A `<meta property="…" content="…">` tag (OpenGraph).
    fn property(property: &str, content: &str) -> HtmlNode {
        Self::meta(HtmlAttr::constant("property"), property, content)
    }

    fn meta(key: HtmlAttr, key_value: &str, content: &str) -> HtmlNode {
        let mut el = HtmlElement::new(tag::meta);
        el.attrs.push(key, key_value);
        el.attrs.push(attr::content, content);
        HtmlNode::Element(el)
    }

    /// A `<link rel="canonical" href="…">` tag.
    fn canonical(href: &str) -> HtmlNode {
        let mut el = HtmlElement::new(tag::link);
        el.attrs.push(attr::rel, "canonical");
        el.attrs.push(attr::href, href);
        HtmlNode::Element(el)
    }
}
