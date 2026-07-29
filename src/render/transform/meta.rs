//! Injects SEO and social meta tags into each page's `<head>`.
//!
//! When `html { meta true }` is set (the default), this appends a description,
//! OpenGraph, Twitter Card, and canonical `<link>` to every page, derived from
//! its frontmatter (`description`/`summary`, `image`, `author`, tags, date) and
//! the site config (`url`, `site`, `author`, `lang`). URL-absolute tags
//! (`og:url`, canonical) are emitted only when a base `url` is configured.
//!
//! typst-html owns the document `<head>` (templates can only set the title), so
//! these tags cannot be authored in a layout, so appending them to the parsed DOM
//! here is the single place they can be added for every page at once.

use typst_html::{HtmlAttr, HtmlDocument, HtmlElement, HtmlNode, attr, tag};

use crate::config::{BaseUrl, Config};
use crate::content::Page;

use super::{Cx, DocumentExt, Transform};
use crate::render::{AssetDeps, AssetMap};

/// OpenGraph names its tags with `property` where the HTML spec uses `name`.
/// typst-html has no constant for it, since it is RDFa rather than HTML.
const PROPERTY: HtmlAttr = HtmlAttr::constant("property");

/// The [`Transform`] that appends meta tags to `<head>`.
pub(super) struct Meta;

impl Transform for Meta {
    fn enabled(&self, config: &Config) -> bool {
        config.html.meta
    }

    fn apply(&self, doc: &mut HtmlDocument, cx: &mut Cx<'_>) {
        let mut card = Card {
            config: cx.config,
            page: cx.page,
            assets: cx.assets,
            probed: AssetDeps::new(),
        };
        let tags = card.tags();
        // The card image resolves through the asset map, so this page depends
        // on where that image is served from.
        cx.found.assets.extend(card.probed);
        if tags.is_empty() {
            return;
        }
        if let Some(head) = doc.head() {
            for node in tags {
                head.children.push(node);
            }
        }
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
    /// The map entries the card image's resolution consulted.
    probed: AssetDeps,
}

/// What a page says about itself, resolved once and then spelled three ways.
struct Facts {
    title: String,
    description: Option<String>,
    /// Already fingerprinted and absolutized, since a social image is read by a
    /// crawler that has no page to resolve a relative URL against.
    image: Option<String>,
    canonical: Option<String>,
    /// The OpenGraph object type.
    kind: &'static str,
}

impl Card<'_> {
    /// This page's generated card, when the build makes one for it. Named from
    /// the permalink alone, so the tag can be written while the image is still
    /// being rendered.
    fn generated_card(&self) -> Option<String> {
        self.page
            .wants_card(self.config)
            .then(|| self.config.generate.cards.url(&self.page.permalink))
    }

    /// Every tag this page carries, in emission order: the plain document meta,
    /// then OpenGraph, then the Twitter card, then the link relations.
    fn tags(&mut self) -> Vec<HtmlNode> {
        let facts = self.facts();
        let mut tags = Vec::new();
        self.document(&facts, &mut tags);
        self.opengraph(&facts, &mut tags);
        Self::twitter(&facts, &mut tags);
        if let Some(url) = &facts.canonical {
            tags.push(Self::canonical(url));
        }
        self.alternates(&mut tags);
        tags
    }

    /// What every vocabulary below says the same thing about, resolved once:
    /// each of the three spells these out differently, and a value computed per
    /// group is a value that can disagree between them.
    fn facts(&mut self) -> Facts {
        let fm = &self.page.frontmatter;
        let (title, description, authored) = (
            fm.title.clone().unwrap_or_default(),
            fm.text("description").or_else(|| fm.text("summary")),
            fm.text("image"),
        );
        // An authored image always wins; a generated card fills in for the
        // pages that have none, which is the whole point of generating them.
        // Resolved before the struct literal because resolution records an
        // asset dependency, and so needs `self` mutably.
        let image = authored.or_else(|| self.generated_card());
        let image = image.map(|src| self.absolute(&src));
        Facts {
            title,
            description,
            image,
            canonical: self.url(),
            // A dated page is an article; everything else is a plain website page.
            kind: match fm.date.is_some() {
                true => "article",
                false => "website",
            },
        }
    }

    /// The document-level tags, which predate every social vocabulary.
    fn document(&self, facts: &Facts, tags: &mut Vec<HtmlNode>) {
        if let Some(description) = &facts.description {
            tags.push(Self::named("description", description));
        }
        let author = self
            .page
            .frontmatter
            .text("author")
            .or_else(|| self.config.author(&self.page.lang).map(str::to_owned));
        if let Some(author) = author {
            tags.push(Self::named("author", &author));
        }
    }

    /// The OpenGraph tags, which is what a link preview reads.
    fn opengraph(&self, facts: &Facts, tags: &mut Vec<HtmlNode>) {
        tags.push(Self::property("og:type", facts.kind));
        if !facts.title.is_empty() {
            tags.push(Self::property("og:title", &facts.title));
        }
        if let Some(description) = &facts.description {
            tags.push(Self::property("og:description", description));
        }
        if let Some(url) = &facts.canonical {
            tags.push(Self::property("og:url", url));
        }
        if self.config.site.is_some() {
            tags.push(Self::property(
                "og:site_name",
                self.config.title(&self.page.lang),
            ));
        }
        tags.push(Self::property("og:locale", &Self::locale(&self.page.lang)));
        if let Some(image) = &facts.image {
            tags.push(Self::property("og:image", image));
        }
    }

    /// The Twitter card tags, which only restate what OpenGraph already said,
    /// bar the card size an image implies.
    fn twitter(facts: &Facts, tags: &mut Vec<HtmlNode>) {
        tags.push(Self::named(
            "twitter:card",
            match facts.image.is_some() {
                true => "summary_large_image",
                false => "summary",
            },
        ));
        if !facts.title.is_empty() {
            tags.push(Self::named("twitter:title", &facts.title));
        }
        if let Some(description) = &facts.description {
            tags.push(Self::named("twitter:description", description));
        }
        if let Some(image) = &facts.image {
            tags.push(Self::named("twitter:image", image));
        }
    }

    /// `<link rel="alternate" hreflang="..">` for each of a translated page's
    /// editions plus an `x-default` to the default language's, so crawlers pair
    /// the translations. Absolute URLs, so gated on a configured base `url`; a
    /// single-language page has no translations and adds none.
    fn alternates(&self, tags: &mut Vec<HtmlNode>) {
        let Some(base) = self.config.base() else {
            return;
        };
        for t in &self.page.translations {
            tags.push(Self::alternate(&t.lang, &base.join(&t.url)));
        }
        if let Some(default) = self
            .page
            .translations
            .iter()
            .find(|t| t.lang == self.config.lang)
        {
            tags.push(Self::alternate("x-default", &base.join(&default.url)));
        }
    }

    /// The page's canonical absolute URL, if a base `url` is configured.
    fn url(&self) -> Option<String> {
        Some(self.config.base()?.join(&self.page.permalink))
    }

    /// Resolve a root-relative asset reference to its fingerprinted URL, then
    /// make it absolute against the site `url`. An already-absolute (`http`)
    /// value, or one with no base URL, is left as authored (bar fingerprinting).
    fn absolute(&mut self, src: &str) -> String {
        let resolved = self.assets.resolve(src);
        self.probed.extend(resolved.probed);
        let src = resolved.url.unwrap_or_else(|| src.to_owned());
        BaseUrl::resolve(self.config.base().as_ref(), &src)
    }

    /// A `<meta name=".." content="..">` tag.
    fn named(name: &str, content: &str) -> HtmlNode {
        Self::meta(attr::name, name, content)
    }

    /// A BCP-47 code as OpenGraph spells a locale: `fr-CA` -> `fr_CA`.
    ///
    /// A bare `fr` is passed through rather than given an invented territory:
    /// `fr_FR` would be wrong for a Belgian or Canadian site, and a guess is
    /// worse than an incomplete tag. Declare the region in `lang` to get the
    /// full form.
    fn locale(code: &str) -> String {
        let mut parts = code.split(['-', '_']);
        let Some(language) = parts.next() else {
            return code.to_owned();
        };
        // A two-letter or three-digit subtag is the region; a four-letter one is
        // the script, which OpenGraph has no place for.
        let region = parts.find(|part| {
            (part.len() == 2 && part.chars().all(|c| c.is_ascii_alphabetic()))
                || (part.len() == 3 && part.chars().all(|c| c.is_ascii_digit()))
        });
        match region {
            Some(region) => format!(
                "{}_{}",
                language.to_ascii_lowercase(),
                region.to_uppercase()
            ),
            None => language.to_ascii_lowercase(),
        }
    }

    /// A `<meta property=".." content="..">` tag (OpenGraph).
    fn property(property: &str, content: &str) -> HtmlNode {
        Self::meta(PROPERTY, property, content)
    }

    fn meta(key: HtmlAttr, key_value: &str, content: &str) -> HtmlNode {
        HtmlElement::new(tag::meta)
            .with_attr(key, key_value)
            .with_attr(attr::content, content)
            .into()
    }

    /// A `<link rel="canonical" href="..">` tag.
    fn canonical(href: &str) -> HtmlNode {
        HtmlElement::new(tag::link)
            .with_attr(attr::rel, "canonical")
            .with_attr(attr::href, href)
            .into()
    }

    /// A `<link rel="alternate" hreflang=".." href="..">` tag.
    fn alternate(hreflang: &str, href: &str) -> HtmlNode {
        HtmlElement::new(tag::link)
            .with_attr(attr::rel, "alternate")
            .with_attr(attr::hreflang, hreflang)
            .with_attr(attr::href, href)
            .into()
    }
}

#[cfg(test)]
mod tests {
    use super::Card;

    #[test]
    fn locale_uses_the_opengraph_separator() {
        assert_eq!(Card::locale("fr-CA"), "fr_CA");
        assert_eq!(Card::locale("pt-br"), "pt_BR");
        assert_eq!(Card::locale("es-419"), "es_419");
    }

    /// A script subtag is not a territory, and a bare code gets no invented one.
    #[test]
    fn locale_leaves_out_what_opengraph_has_no_place_for() {
        assert_eq!(Card::locale("zh-Hant"), "zh");
        assert_eq!(Card::locale("zh-Hant-TW"), "zh_TW");
        assert_eq!(Card::locale("fr"), "fr");
    }
}
