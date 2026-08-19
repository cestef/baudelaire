//! Injects SEO and social meta tags into each page's `<head>`, derived from its
//! frontmatter and the site config. URL-absolute tags (`og:url`, canonical) are
//! emitted only when a base `url` is configured.

use typst_html::{HtmlAttr, HtmlDocument, HtmlElement, HtmlNode, attr, tag};

use crate::config::{BaseUrl, Config, ManifestConfig};
use crate::content::entities::{Attribution, Byline, Credit, Vocabulary};
use crate::content::{Iso, Page};

use super::{Cx, DocumentExt, PROPERTY, Transform};
use crate::render::{AssetDeps, AssetMap};

/// The [`Transform`] that appends meta tags to `<head>`.
pub(super) struct Meta;

impl Transform for Meta {
    fn enabled(&self, config: &Config) -> bool {
        config.html.meta.enabled
    }

    fn apply(&self, doc: &mut HtmlDocument, cx: &mut Cx<'_>) {
        let byline = Byline::of(cx.entities, cx.config, cx.page).or_site(cx.config, cx.page);
        let mut card = Card {
            config: cx.config,
            page: cx.page,
            related: cx.related,
            assets: cx.assets,
            probed: AssetDeps::new(),
        };
        let tags = card.tags(&byline);
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
    /// The page's editions in other languages, which the `hreflang` alternates
    /// name.
    related: &'a crate::content::Related,
    /// Processed-asset URL map, consulted here because the fingerprint
    /// transform runs later and cannot resolve an absolute `content` value.
    assets: &'a AssetMap,
    /// The map entries the card image's resolution consulted.
    probed: AssetDeps,
}

/// What a page says about itself, resolved once and then spelled three ways.
struct Facts {
    title: String,
    description: Option<String>,
    /// Already fingerprinted and absolutized, since a crawler has no page to
    /// resolve a relative URL against.
    image: Option<String>,
    /// What the image shows; a *generated* card falls back to the page title,
    /// which is what the card renders.
    alt: Option<String>,
    canonical: Option<String>,
    /// The OpenGraph object type.
    kind: &'static str,
    /// When the page was published and when it last changed, as ISO-8601 days.
    published: Option<String>,
    modified: Option<String>,
    /// Who the page credits, by role, already resolved through each registry's
    /// slots and with every picture named at the URL it is served from.
    byline: Byline,
    /// Every taxonomy term the page carries, flattened across taxonomies.
    terms: Vec<String>,
}

impl Card<'_> {
    /// This page's generated card, when the build makes one for it. Named from
    /// the permalink alone, so the tag can be written while the image is still
    /// being rendered.
    fn generated_card(&self) -> Option<String> {
        self.page
            .wants_card(self.config)
            .then(|| self.config.artifacts.cards.url(&self.page.permalink))
    }

    /// Every tag this page carries, in emission order: the plain document meta,
    /// then OpenGraph, then the Twitter card, then the link relations.
    fn tags(&mut self, byline: &Byline) -> Vec<HtmlNode> {
        let facts = self.facts(byline);
        let mut tags = Vec::new();
        Self::document(&facts, &mut tags);
        self.opengraph(&facts, &mut tags);
        self.twitter(&facts, &mut tags);
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
    /// different things about one page. Every `<` becomes its JSON escape: a
    /// title carrying `</script>` would close the island early, and one
    /// carrying `<!--<script` would swallow the rest of the page.
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
        for (role, credited) in facts.byline.roles() {
            let Some(property) = role.spelling(Vocabulary::JsonLd) else {
                continue;
            };
            let people: Vec<serde_json::Value> = credited.iter().map(Self::person).collect();
            fields.push((property, people.into()));
        }
        if !facts.terms.is_empty() {
            fields.push(("keywords", facts.terms.clone().into()));
        }
        let object: serde_json::Map<String, serde_json::Value> = fields
            .into_iter()
            .map(|(key, value)| (key.to_owned(), value))
            .collect();
        let json = serde_json::to_string(&object).expect("plain strings");
        let json = json.replace('<', "\\u003c");
        let mut el = HtmlElement::new(tag::script).with_attr(attr::r#type, "application/ld+json");
        el.children
            .push(HtmlNode::Text(json.into(), typst::syntax::Span::detached()));
        el.into()
    }

    /// One credited entity as schema.org describes it, typed from the
    /// registry's shape rather than always as a person.
    fn person(credited: &Attribution) -> serde_json::Value {
        let mut object = serde_json::Map::new();
        object.insert("@type".into(), credited.kind.into());
        object.insert("name".into(), credited.display.clone().into());
        for (key, value) in [
            ("url", credited.url.as_deref()),
            ("image", credited.image.as_deref()),
            ("email", credited.email.as_deref()),
        ] {
            if let Some(value) = value {
                object.insert(key.to_owned(), value.into());
            }
        }
        if !credited.same_as.is_empty() {
            object.insert("sameAs".into(), credited.same_as.clone().into());
        }
        object.into()
    }

    /// The feed autodiscovery links: one per configured format, pointing at the
    /// feed for this page's language and at its own collection's, where it has
    /// one.
    fn feeds(&self, tags: &mut Vec<HtmlNode>) {
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
        let Some(own) = self.config.channel(self.page.section(), &self.page.lang) else {
            return;
        };
        advertise(&own.scope, &own.title, tags);
    }

    /// The `<link rel="manifest">` pointing at this page's language's manifest,
    /// and the `theme-color` that manifest declares.
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

    fn facts(&mut self, byline: &Byline) -> Facts {
        let fm = &self.page.frontmatter;
        let (title, description, authored) = (
            fm.title.clone().unwrap_or_default(),
            fm.blurb().map(str::to_owned),
            fm.image.clone(),
        );
        let card = match authored {
            Some(_) => None,
            None => self.generated_card(),
        };
        let image = authored
            .or_else(|| card.clone())
            .or_else(|| self.config.html.meta.image.clone());
        let image = image.map(|src| self.absolute(&src));
        let alt = fm
            .alt
            .clone()
            .or_else(|| card.is_some().then(|| title.clone()))
            .filter(|alt| !alt.is_empty());
        Facts {
            title,
            description,
            image,
            alt,
            canonical: self.url(),
            kind: if fm.date.is_some() {
                "article"
            } else {
                "website"
            },
            published: fm.date.map(|d| Iso(d).to_string()),
            modified: fm.updated.map(|d| Iso(d).to_string()),
            byline: byline.clone().images(|src| self.absolute(src)),
            terms: fm.taxonomies.values().flatten().cloned().collect(),
        }
    }

    /// The document-level tags, which predate every social vocabulary.
    fn document(facts: &Facts, tags: &mut Vec<HtmlNode>) {
        if let Some(description) = &facts.description {
            tags.push(Self::named("description", description));
        }
        let Some(name) = Credit::Author.spelling(Vocabulary::Meta) else {
            return;
        };
        for author in facts.byline.authors() {
            tags.push(Self::named(name, &author.display));
        }
        if let Some(url) = facts.byline.authors().iter().find_map(|a| a.url.as_deref()) {
            tags.push(
                HtmlElement::new(tag::link)
                    .with_attr(attr::rel, "author")
                    .with_attr(attr::href, url)
                    .into(),
            );
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
        if facts.kind == "article" {
            for (property, value) in [
                ("article:published_time", facts.published.as_deref()),
                ("article:modified_time", facts.modified.as_deref()),
            ] {
                if let Some(value) = value {
                    tags.push(Self::property(property, value));
                }
            }
            if let Some(property) = Credit::Author.spelling(Vocabulary::OpenGraph) {
                for author in facts.byline.authors() {
                    let value = author.url.as_deref().unwrap_or(&author.display);
                    tags.push(Self::property(property, value));
                }
            }
            for term in &facts.terms {
                tags.push(Self::property("article:tag", term));
            }
        }
    }

    /// The Twitter card tags, which only restate what OpenGraph already said,
    /// bar the card size an image implies and the account the site names.
    fn twitter(&self, facts: &Facts, tags: &mut Vec<HtmlNode>) {
        if let Some(handle) = &self.config.html.meta.twitter {
            tags.push(Self::named("twitter:site", handle));
        }
        tags.push(Self::named(
            "twitter:card",
            if facts.image.is_some() {
                "summary_large_image"
            } else {
                "summary"
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
    /// the translations. Absolute URLs, so gated on a configured base `url`.
    fn alternates(&self, tags: &mut Vec<HtmlNode>) {
        let Some(base) = self.config.base() else {
            return;
        };
        for t in &self.related.translations {
            tags.push(Self::alternate(&t.lang, &base.join(&t.url)));
        }
        if let Some(default) = self
            .related
            .translations
            .iter()
            .find(|t| t.lang == self.config.lang)
        {
            tags.push(Self::alternate("x-default", &base.join(&default.url)));
        }
    }

    /// `<link rel="alternate" type="application/pdf">` to this page's PDF,
    /// gated on the same [`Page::wants_pdf`] the exporter reads so a page that
    /// gets no PDF cannot advertise one.
    fn pdf(&self, tags: &mut Vec<HtmlNode>) {
        if !self.page.wants_pdf(self.config) {
            return;
        }
        let href = self.config.artifacts.pdf.pages.url(&self.page.permalink);
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
    /// value, or one with no base URL, is left as authored bar the fingerprint.
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

    /// A BCP-47 code as OpenGraph spells a locale: `fr-CA` -> `fr_CA`. A code
    /// naming no region is passed through rather than given an invented one.
    fn locale(code: &str) -> String {
        let mut parts = code.split(['-', '_']);
        let Some(language) = parts.next() else {
            return code.to_owned();
        };
        let region = parts.find(|part| {
            (part.len() == 2 && part.chars().all(|c| c.is_ascii_alphabetic()))
                || (part.len() == 3 && part.chars().all(|c| c.is_ascii_digit()))
        });
        region.map_or_else(
            || language.to_ascii_lowercase(),
            |region| {
                format!(
                    "{}_{}",
                    language.to_ascii_lowercase(),
                    region.to_uppercase()
                )
            },
        )
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
    use super::{Attribution, Byline, Card, Credit, Facts};
    use typst_html::{HtmlNode, attr};

    fn credited(display: &str, url: Option<&str>, same_as: Vec<&str>) -> Attribution {
        Attribution {
            display: display.into(),
            url: url.map(str::to_owned),
            image: None,
            email: None,
            same_as: same_as.into_iter().map(str::to_owned).collect(),
            kind: "Person",
            fields: crate::codegen::Value::None,
        }
    }

    fn byline(authors: Vec<Attribution>) -> Byline {
        let mut byline = Byline::default();
        byline.push(Credit::Author, authors);
        byline
    }

    /// The facts of a page that says nothing but its byline and its kind.
    fn facts(byline: &Byline, kind: &'static str) -> Facts {
        Facts {
            title: "T".into(),
            description: None,
            image: None,
            alt: None,
            canonical: None,
            kind,
            published: None,
            modified: None,
            byline: byline.clone(),
            terms: Vec::new(),
        }
    }

    /// The `content` of every `<meta name="..">` tag of one name, in order.
    fn named(tags: &[HtmlNode], name: &str) -> Vec<String> {
        tags.iter()
            .filter_map(|node| match node {
                HtmlNode::Element(el) => Some(el),
                _ => None,
            })
            .filter(|el| el.attrs.get(attr::name).is_some_and(|it| it == name))
            .filter_map(|el| el.attrs.get(attr::content).map(ToString::to_string))
            .collect()
    }

    /// The `href` of the one `<link rel="..">` of a relation.
    fn rel(tags: &[HtmlNode], relation: &str) -> Option<String> {
        tags.iter()
            .filter_map(|node| match node {
                HtmlNode::Element(el) => Some(el),
                _ => None,
            })
            .find(|el| el.attrs.get(attr::rel).is_some_and(|it| it == relation))
            .and_then(|el| el.attrs.get(attr::href).map(ToString::to_string))
    }

    /// The JSON-LD island a page carries, parsed back.
    fn island(facts: &Facts) -> serde_json::Value {
        let HtmlNode::Element(el) = Card::jsonld(facts) else {
            panic!("the island is an element")
        };
        let HtmlNode::Text(json, _) = &el.children[0] else {
            panic!("the island holds its object as text")
        };
        serde_json::from_str(json).expect("valid JSON")
    }

    #[test]
    fn locale_uses_the_opengraph_separator() {
        assert_eq!(Card::locale("fr-CA"), "fr_CA");
        assert_eq!(Card::locale("pt-br"), "pt_BR");
        assert_eq!(Card::locale("es-419"), "es_419");
    }

    #[test]
    fn locale_leaves_out_what_opengraph_has_no_place_for() {
        assert_eq!(Card::locale("zh-Hant"), "zh");
        assert_eq!(Card::locale("zh-Hant-TW"), "zh_TW");
        assert_eq!(Card::locale("fr"), "fr");
    }

    #[test]
    fn the_document_tags_name_every_credited_author() {
        let byline = byline(vec![
            credited("Camille", Some("https://camille.example"), Vec::new()),
            credited("Zoe", None, Vec::new()),
        ]);
        let facts = facts(&byline, "article");
        let mut tags = Vec::new();

        Card::document(&facts, &mut tags);

        assert_eq!(named(&tags, "author"), ["Camille", "Zoe"]);
        assert_eq!(
            rel(&tags, "author").as_deref(),
            Some("https://camille.example")
        );
    }

    #[test]
    fn the_json_island_types_every_role_it_can_spell() {
        let mut byline = byline(vec![credited(
            "Camille",
            Some("https://camille.example"),
            vec!["https://social.example/@camille"],
        )]);
        byline.push(
            Credit::Translator,
            vec![Attribution {
                kind: "Organization",
                ..credited("Zoe", None, Vec::new())
            }],
        );
        let json = island(&facts(&byline, "article"));

        assert_eq!(json["author"][0]["@type"], "Person");
        assert_eq!(json["author"][0]["name"], "Camille");
        assert_eq!(json["author"][0]["url"], "https://camille.example");
        assert_eq!(
            json["author"][0]["sameAs"][0],
            "https://social.example/@camille"
        );
        assert_eq!(json["translator"][0]["name"], "Zoe");
        assert_eq!(json["translator"][0]["@type"], "Organization");
    }

    /// Both ways out of the island: `</script>` closes it early, `<!--<script`
    /// opens the double-escaped state after which its own closing tag is not
    /// read as one.
    #[test]
    fn a_title_cannot_close_the_json_island() {
        let byline = Byline::default();
        let mut facts = facts(&byline, "website");
        facts.title = "Escaping </script> in typst".into();
        facts.description = Some("A comment opener, <!--<script, is the other way in".into());
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
        let parsed: serde_json::Value = serde_json::from_str(json).expect("valid JSON");
        assert_eq!(parsed["headline"], "Escaping </script> in typst");
        assert_eq!(parsed["@type"], "WebPage");
    }
}
