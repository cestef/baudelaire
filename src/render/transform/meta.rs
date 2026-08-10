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
        // The site's own author is the floor here: `<meta name="author">` on a
        // page that names nobody is the site's, which is what it has always
        // been.
        let (byline, _) = Byline::of(cx.entities, cx.config, cx.page);
        let byline = byline.or_site(cx.config, cx.page);
        let mut card = Card {
            config: cx.config,
            page: cx.page,
            assets: cx.assets,
            probed: AssetDeps::new(),
        };
        let tags = card.tags(&byline);
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
struct Facts<'a> {
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
    /// Who the page credits, by role, already resolved through each registry's
    /// slots. Every vocabulary below reads this one answer: two resolutions are
    /// two chances to disagree, and these two did.
    byline: &'a Byline,
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
    /// different things about one page. An `Article` where the page is dated,
    /// a `WebPage` otherwise, which is the same split `og:type` makes.
    fn jsonld(facts: &Facts<'_>) -> HtmlNode {
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
        // Every role the island can spell, as an array of typed objects: the
        // one vocabulary rich enough to say who translated a page, and the only
        // one that can carry a link to them.
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

    /// One credited entity as schema.org describes it.
    ///
    /// Typed from the registry's shape, so an `organizations` registry is an
    /// `Organization` rather than a person with a logo. `sameAs` is what the
    /// vocabulary calls "the same thing, elsewhere", which is exactly what a
    /// socials field holds.
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
    fn facts<'b>(&mut self, byline: &'b Byline) -> Facts<'b> {
        let fm = &self.page.frontmatter;
        let (title, description, authored) = (
            fm.title.clone().unwrap_or_default(),
            fm.blurb().map(str::to_owned),
            fm.image.clone(),
        );
        // An authored image always wins; a generated card fills in for the
        // pages that have none, which is the whole point of generating them.
        // Resolved before the struct literal because resolution records an
        // asset dependency, and so needs `self` mutably.
        let card = match authored {
            Some(_) => None,
            None => self.generated_card(),
        };
        // The page's own, else the card the build drew it, else the site's
        // floor. Last because it is a floor: a site image on a page that has one
        // of its own would preview the wrong thing.
        let image = authored
            .or_else(|| card.clone())
            .or_else(|| self.config.html.meta.image.clone());
        let image = image.map(|src| self.absolute(&src));
        // A generated card draws the page title, so that is a true description
        // of it. An authored image is the author's to describe, and the site's
        // floor image shows whatever it shows: it is one picture standing in for
        // every page, so a page title describes it only by coincidence.
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
            byline,
            terms: fm.taxonomies.values().flatten().cloned().collect(),
        }
    }

    /// The document-level tags, which predate every social vocabulary.
    ///
    /// Reads [`Facts`] like every other vocabulary rather than resolving the
    /// author a second time: two resolutions are two chances to disagree, and
    /// these two did.
    fn document(facts: &Facts<'_>, tags: &mut Vec<HtmlNode>) {
        if let Some(description) = &facts.description {
            tags.push(Self::named("description", description));
        }
        let Some(name) = Credit::Author.spelling(Vocabulary::Meta) else {
            return;
        };
        // One tag per credited author. The document vocabulary has no way to
        // relate two of them, so a co-authored page repeats the tag rather than
        // joining the names into a string no consumer can split back.
        for author in facts.byline.authors() {
            tags.push(Self::named(name, &author.display));
        }
        // Where an author has a page of their own, say so: `rel="author"` is
        // how a reader-mode or a feed reader finds the person rather than the
        // string. Only the first, since the relation is singular.
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
    fn opengraph(&self, facts: &Facts<'_>, tags: &mut Vec<HtmlNode>) {
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
            ] {
                if let Some(value) = value {
                    tags.push(Self::property(property, value));
                }
            }
            // OpenGraph wants a profile URL here and takes a name where there
            // is none, which is all a site without a roster ever had. One per
            // author: the vocabulary repeats the property rather than joining.
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
    fn twitter(&self, facts: &Facts<'_>, tags: &mut Vec<HtmlNode>) {
        // Whose site this is. Nothing on the page says it, so without the
        // config a card attributes the link to whoever posted it and to nobody
        // else.
        if let Some(handle) = &self.config.html.meta.twitter {
            tags.push(Self::named("twitter:site", handle));
        }
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
    use super::{Attribution, Byline, Card, Credit, Facts};
    use typst_html::{HtmlNode, attr};

    /// One credited entity: a name, and whatever of the optional halves the
    /// case is about.
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

    /// A byline crediting `authors` and nothing else.
    fn byline(authors: Vec<Attribution>) -> Byline {
        let mut byline = Byline::default();
        byline.push(Credit::Author, authors);
        byline
    }

    /// The facts of a page that says nothing but its byline and its kind.
    fn facts<'a>(byline: &'a Byline, kind: &'static str) -> Facts<'a> {
        Facts {
            title: "T".into(),
            description: None,
            image: None,
            alt: None,
            canonical: None,
            kind,
            published: None,
            modified: None,
            byline,
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
    fn island(facts: &Facts<'_>) -> serde_json::Value {
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

    /// A script subtag is not a territory, and a bare code gets no invented one.
    #[test]
    fn locale_leaves_out_what_opengraph_has_no_place_for() {
        assert_eq!(Card::locale("zh-Hant"), "zh");
        assert_eq!(Card::locale("zh-Hant-TW"), "zh_TW");
        assert_eq!(Card::locale("fr"), "fr");
    }

    /// One resolved byline, spelled by every vocabulary. The document tag used
    /// to resolve its own author, language-aware, while [`super::Facts`]
    /// resolved the bare site-wide field: on a site with
    /// `languages { fr { author .. } }` the two named different people on the
    /// same page.
    #[test]
    fn the_document_tags_name_every_credited_author() {
        let byline = byline(vec![
            credited("Camille", Some("https://camille.example"), Vec::new()),
            credited("Zoe", None, Vec::new()),
        ]);
        let facts = facts(&byline, "article");
        let mut tags = Vec::new();

        Card::document(&facts, &mut tags);

        // One `<meta name="author">` each, and one `rel="author"` for the one
        // that has a page of their own.
        assert_eq!(named(&tags, "author"), ["Camille", "Zoe"]);
        assert_eq!(
            rel(&tags, "author").as_deref(),
            Some("https://camille.example")
        );
    }

    /// The schema.org island is the only vocabulary that can say a role other
    /// than author, and the only one that can carry a link to the person.
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
        // A role the island can spell but no other vocabulary can.
        assert_eq!(json["translator"][0]["name"], "Zoe");
        assert_eq!(json["translator"][0]["@type"], "Organization");
    }

    /// A title carrying `</script>` would close the island early and spill the
    /// rest of the object into the document as text; one carrying `<!--<script`
    /// would open the double-escaped state instead, after which the island's own
    /// closing tag is not read as one and the rest of the page is swallowed.
    /// Escaping every `<` closes both, and JSON reads it back as the same
    /// character.
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
        // ...and it is still the JSON it claims to be.
        let parsed: serde_json::Value = serde_json::from_str(json).expect("valid JSON");
        assert_eq!(parsed["headline"], "Escaping </script> in typst");
        assert_eq!(parsed["@type"], "WebPage");
    }
}
