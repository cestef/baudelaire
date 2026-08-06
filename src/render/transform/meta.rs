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

use crate::config::{BaseUrl, Config, ManifestConfig};
use crate::content::{Iso, Page};

use super::{Cx, DocumentExt, PROPERTY, Transform};
use crate::render::{AssetDeps, AssetMap};

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
    /// What the image shows, for a reader who cannot see it. Authored as `alt`
    /// beside `image`; a *generated* card falls back to the page title, which
    /// is what the card renders.
    alt: Option<String>,
    canonical: Option<String>,
    /// The OpenGraph object type.
    kind: &'static str,
    /// When the page was published and when it last changed, as ISO-8601 days.
    /// `article:*` and JSON-LD both read them, so they are resolved once.
    published: Option<String>,
    modified: Option<String>,
    /// The page's author, else the site's.
    author: Option<String>,
    /// Every taxonomy term the page carries, flattened: an `article:tag` does
    /// not distinguish which taxonomy a term came from.
    terms: Vec<String>,
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
        Self::document(&facts, &mut tags);
        self.opengraph(&facts, &mut tags);
        Self::twitter(&facts, &mut tags);
        if let Some(url) = &facts.canonical {
            tags.push(Self::canonical(url));
        }
        self.alternates(&mut tags);
        self.feeds(&mut tags);
        self.manifest(&mut tags);
        self.pdf(&mut tags);
        if self.config.html.jsonld {
            tags.push(Self::jsonld(&facts));
        }
        tags
    }

    /// The schema.org description of this page, as a JSON-LD island.
    ///
    /// Built from the same [`Facts`] the meta tags are, so the two cannot claim
    /// different things about one page. An `Article` where the page is dated,
    /// a `WebPage` otherwise, which is the same split `og:type` makes.
    fn jsonld(facts: &Facts) -> HtmlNode {
        let mut fields: Vec<(&str, serde_json::Value)> = vec![
            ("@context", "https://schema.org".into()),
            (
                "@type",
                match facts.kind {
                    "article" => "Article",
                    _ => "WebPage",
                }
                .into(),
            ),
            ("headline", facts.title.clone().into()),
        ];
        for (key, value) in [
            ("description", facts.description.as_deref()),
            ("image", facts.image.as_deref()),
            ("url", facts.canonical.as_deref()),
            ("datePublished", facts.published.as_deref()),
            ("dateModified", facts.modified.as_deref()),
        ] {
            if let Some(value) = value {
                fields.push((key, value.into()));
            }
        }
        if let Some(author) = &facts.author {
            fields.push((
                "author",
                serde_json::json!({ "@type": "Person", "name": author }),
            ));
        }
        if !facts.terms.is_empty() {
            fields.push(("keywords", facts.terms.clone().into()));
        }
        let object: serde_json::Map<String, serde_json::Value> = fields
            .into_iter()
            .map(|(key, value)| (key.to_owned(), value))
            .collect();
        // Infallible: every value is a string, a list of them, or a map of the
        // same. `serde_json` only fails on what cannot be a JSON key.
        let json = serde_json::to_string(&object).expect("plain strings");
        // A title or description carrying `</script>` would close the island
        // early and spill the rest into the document as text, and `<!--<script`
        // would do worse: it puts the tokenizer into the script-data
        // double-escaped state, where the island's own `</script>` stops closing
        // it and the remainder of the page is swallowed. Escaping `</` alone
        // shut only the first door. Every `<` becomes its JSON escape for
        // U+003C, which parses back to the same character and closes the class;
        // it is safe wholesale because `<` only ever occurs inside a string
        // here, never in the JSON structure.
        let json = json.replace('<', "\\u003c");
        let mut el = HtmlElement::new(tag::script).with_attr(attr::r#type, "application/ld+json");
        el.children
            .push(HtmlNode::Text(json.into(), typst::syntax::Span::detached()));
        el.into()
    }

    /// The feed autodiscovery links: one per configured format, pointing at the
    /// feed for this page's language.
    ///
    /// This is how a reader, a browser extension, or a subscribe button finds a
    /// feed at all. Without it the feeds were written and nothing pointed at
    /// them, and since typst-html owns `<head>` an author could not add the tag
    /// in a layout either.
    fn feeds(&self, tags: &mut Vec<HtmlNode>) {
        // Feeds are absolute-URL artifacts and refuse to generate without a
        // base, so a missing one means there is no feed to point at.
        let Some(base) = self.config.base() else {
            return;
        };
        let site = self.config.title(&self.page.lang);
        let advertise = |scope: &str, title: &str, tags: &mut Vec<HtmlNode>| {
            for kind in &self.config.generate.feed.formats {
                let href = self.config.generate.feed.url(*kind, &base, scope);
                tags.push(
                    HtmlElement::new(tag::link)
                        .with_attr(attr::rel, "alternate")
                        .with_attr(attr::r#type, kind.mime())
                        .with_attr(attr::title, title)
                        .with_attr(attr::href, &href)
                        .into(),
                );
            }
        };
        advertise(&self.config.scope(&self.page.lang, ""), site, tags);
        // A page in a collection that carries its own feed advertises that one
        // too, which is the whole point of having it: a reader on a post is
        // offered the posts, not the everything. Both the location and the name
        // come from the config, which is what keeps this tag and the file the
        // feed processor wrote from disagreeing. It reads only the page's own
        // collection, so it widens no page's cache identity.
        let Some(own) = self.config.channel(self.page.section(), &self.page.lang) else {
            return;
        };
        advertise(&own.scope, &own.title, tags);
    }

    /// The `<link rel="manifest">` pointing at this page's language's manifest,
    /// and the `theme-color` that manifest declares.
    ///
    /// Without the link the file is written and nothing reads it: a browser
    /// learns a site is installable from the page, not from the file's presence.
    /// The colour is repeated as a meta tag because it tints the browser UI on
    /// an ordinary visit too, long before anyone installs anything.
    fn manifest(&self, tags: &mut Vec<HtmlNode>) {
        let manifest = &self.config.generate.manifest;
        if !manifest.enabled {
            return;
        }
        tags.push(
            HtmlElement::new(tag::link)
                .with_attr(attr::rel, "manifest")
                .with_attr(
                    attr::href,
                    ManifestConfig::url(self.config, &self.page.lang),
                )
                .into(),
        );
        if let Some(theme) = &manifest.theme {
            tags.push(Self::named("theme-color", theme));
        }
    }

    /// What every vocabulary below says the same thing about, resolved once:
    /// each of the three spells these out differently, and a value computed per
    /// group is a value that can disagree between them.
    fn facts(&mut self) -> Facts {
        let fm = &self.page.frontmatter;
        let (title, description, authored) = (
            fm.title.clone().unwrap_or_default(),
            fm.description(),
            fm.text("image"),
        );
        // An authored image always wins; a generated card fills in for the
        // pages that have none, which is the whole point of generating them.
        // Resolved before the struct literal because resolution records an
        // asset dependency, and so needs `self` mutably.
        let generated = authored.is_none();
        let image = authored.or_else(|| self.generated_card());
        let image = image.map(|src| self.absolute(&src));
        // A generated card draws the page title, so that is a true description
        // of it. An authored image is the author's to describe.
        let alt = fm
            .text("alt")
            .or_else(|| (generated && image.is_some()).then(|| title.clone()))
            .filter(|alt| !alt.is_empty());
        Facts {
            title,
            description,
            image,
            alt,
            canonical: self.url(),
            // A dated page is an article; everything else is a plain website page.
            kind: match fm.date.is_some() {
                true => "article",
                false => "website",
            },
            published: fm.date.map(|d| Iso(d).to_string()),
            // Only when it actually moved: `modified` falls back to the publish
            // date, and restating that as a modification says nothing.
            modified: fm.updated.map(|d| Iso(d).to_string()),
            // The site's answer for *this page's language*: a
            // `languages { fr { author .. } }` site read the bare field here
            // and the language-aware one where the document tag was written, so
            // `<meta name="author">` and `article:author` named two different
            // people on the same page.
            author: fm
                .text("author")
                .or_else(|| self.config.author(&self.page.lang).map(str::to_owned)),
            terms: fm.taxonomies.values().flatten().cloned().collect(),
        }
    }

    /// The document-level tags, which predate every social vocabulary.
    ///
    /// Reads [`Facts`] like every other vocabulary rather than resolving the
    /// author a second time: two resolutions are two chances to disagree, and
    /// these two did.
    fn document(facts: &Facts, tags: &mut Vec<HtmlNode>) {
        if let Some(description) = &facts.description {
            tags.push(Self::named("description", description));
        }
        if let Some(author) = &facts.author {
            tags.push(Self::named("author", author));
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
            if let Some(alt) = &facts.alt {
                tags.push(Self::property("og:image:alt", alt));
            }
        }
        // Only an article has an article vocabulary. A website page carries no
        // publication date, which is what made it a website page.
        if facts.kind == "article" {
            for (property, value) in [
                ("article:published_time", facts.published.as_deref()),
                ("article:modified_time", facts.modified.as_deref()),
                ("article:author", facts.author.as_deref()),
            ] {
                if let Some(value) = value {
                    tags.push(Self::property(property, value));
                }
            }
            for term in &facts.terms {
                tags.push(Self::property("article:tag", term));
            }
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

    /// `<link rel="alternate" type="application/pdf">` to this page's PDF.
    ///
    /// The file is written by the build that compiles the page, so the tag and
    /// the exporter derive the URL the same way, from [`Page::wants_pdf`]: a
    /// page that gets no PDF must not advertise one. Root-relative, unlike the
    /// feeds: a PDF beside the page needs no base URL to be reachable.
    fn pdf(&self, tags: &mut Vec<HtmlNode>) {
        if !self.page.wants_pdf(self.config) {
            return;
        }
        let href = self.config.generate.pdf.pages.url(&self.page.permalink);
        tags.push(
            HtmlElement::new(tag::link)
                .with_attr(attr::rel, "alternate")
                .with_attr(attr::r#type, crate::mime::Mime::PDF)
                .with_attr(
                    attr::href,
                    BaseUrl::resolve(self.config.base().as_ref(), &href),
                )
                .into(),
        );
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

    /// Every vocabulary reads one resolved author. The document tag resolved
    /// its own, language-aware, while [`super::Facts`] resolved the bare
    /// site-wide field: on a site with `languages { fr { author .. } }` the two
    /// named different people on the same page.
    #[test]
    fn the_document_author_is_the_one_the_facts_resolved() {
        let facts = super::Facts {
            title: "T".into(),
            description: None,
            image: None,
            alt: None,
            canonical: None,
            kind: "article",
            published: None,
            modified: None,
            author: Some("Camille".into()),
            terms: Vec::new(),
        };
        let mut tags = Vec::new();

        Card::document(&facts, &mut tags);

        assert_eq!(
            tags.len(),
            1,
            "a description-less page tags only its author"
        );
        let typst_html::HtmlNode::Element(el) = &tags[0] else {
            panic!("expected an element")
        };
        assert_eq!(
            el.attrs
                .get(typst_html::attr::name)
                .map(typst::ecow::EcoString::as_str),
            Some("author")
        );
        assert_eq!(
            el.attrs
                .get(typst_html::attr::content)
                .map(typst::ecow::EcoString::as_str),
            Some("Camille")
        );
    }

    /// A title carrying `</script>` would close the island early and spill the
    /// rest of the object into the document as text; one carrying `<!--<script`
    /// would open the double-escaped state instead, after which the island's own
    /// closing tag is not read as one and the rest of the page is swallowed.
    /// Escaping every `<` closes both, and JSON reads it back as the same
    /// character.
    #[test]
    fn a_title_cannot_close_the_json_island() {
        let facts = super::Facts {
            title: "Escaping </script> in typst".into(),
            description: Some("A comment opener, <!--<script, is the other way in".into()),
            image: None,
            alt: None,
            canonical: None,
            kind: "website",
            published: None,
            modified: None,
            author: None,
            terms: Vec::new(),
        };
        let node = Card::jsonld(&facts);
        let typst_html::HtmlNode::Element(el) = node else {
            panic!("expected an element")
        };
        let typst_html::HtmlNode::Text(json, _) = &el.children[0] else {
            panic!("expected text")
        };
        assert!(!json.contains("</script"), "{json}");
        assert!(!json.contains("<!--"), "{json}");
        assert!(json.contains("\\u003c/script"), "{json}");
        // ...and it is still the JSON it claims to be.
        let parsed: serde_json::Value = serde_json::from_str(json).expect("valid JSON");
        assert_eq!(parsed["headline"], "Escaping </script> in typst");
        assert_eq!(parsed["@type"], "WebPage");
    }
}
