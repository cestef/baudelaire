//! The `baudelaire:*` modules serving the site's own build data: identity,
//! config constants, the asset map, and the page-derived tables.
//!
//! Each is [`Esm`]-shaped, so what a module exports and what it declares are
//! two renderings of one value rather than two hand-kept lists.

use std::collections::BTreeMap;

use crate::codegen::{Js, Value};
use crate::content::{Data, Iso, Page};
use crate::engine::emit::dts::Dts;
use crate::world::BuildContext;

use super::{Esm, Module, ModuleCx};

/// The declarations whose shape is fixed rather than read off site data: a
/// table's rows are the same on an empty site as on a full one.
const ASSETS: &str = include_str!("types/assets.d.ts");
const PAGES: &str = include_str!("types/pages.d.ts");
const SECTIONS: &str = include_str!("types/sections.d.ts");
const TAXONOMIES: &str = include_str!("types/taxonomies.d.ts");
const FEED: &str = include_str!("types/feed.d.ts");

/// `baudelaire:site`: site identity and build version, mirroring what templates
/// read from `sys.inputs.baudelaire`.
///
/// The field list belongs to [`BuildContext::site_fields`], which the Typst
/// module `@baudelaire/site` serves too, so the two cannot drift.
pub(super) struct Site;

impl Module for Site {
    fn entries(&self, cx: &ModuleCx) -> Vec<(String, String)> {
        let fields = BuildContext::site_fields(cx.context);
        vec![("baudelaire:site".into(), Esm::object(&Value::Dict(fields)))]
    }

    fn types(&self, cx: &ModuleCx) -> Vec<(String, Dts)> {
        let fields = BuildContext::site_fields(cx.context);
        vec![("baudelaire:site".into(), Esm::typed(&Value::Dict(fields)))]
    }
}

/// `baudelaire:config`: user-defined build-time constants from the `client { }`
/// config block: baudelaire's answer to Vite's `define`.
pub(super) struct Settings;

impl Settings {
    /// The build context's `client` sub-tree: same source as
    /// `sys.inputs.baudelaire.client`.
    fn data(cx: &ModuleCx) -> Value {
        cx.context
            .get("client")
            .cloned()
            .unwrap_or_else(|| Value::dict::<&str>([]))
    }
}

impl Module for Settings {
    fn entries(&self, cx: &ModuleCx) -> Vec<(String, String)> {
        vec![("baudelaire:config".into(), Esm::object(&Self::data(cx)))]
    }

    fn types(&self, cx: &ModuleCx) -> Vec<(String, Dts)> {
        // Typed from the block itself, so an editor completes this site's own
        // constants rather than an anonymous bag of unknowns.
        vec![("baudelaire:config".into(), Esm::typed(&Self::data(cx)))]
    }
}

/// `baudelaire:assets`: the request -> fingerprinted-URL map, with a `url()`
/// lookup that falls back to the input, so client JS can reference a hashed
/// asset by its logical path, the way CSS and HTML already do.
pub(super) struct Assets;

impl Module for Assets {
    fn entries(&self, cx: &ModuleCx) -> Vec<(String, String)> {
        let map = Value::dict(cx.assets.iter().map(|(from, to)| (from, Value::str(to))));
        let src = format!(
            "const map = {};\n\
             export function url(path) {{ return map[path] ?? path; }}\n\
             export default map;\n",
            Js(&map)
        );
        vec![("baudelaire:assets".into(), src)]
    }

    fn types(&self, _cx: &ModuleCx) -> Vec<(String, Dts)> {
        vec![("baudelaire:assets".into(), Dts::new().part(ASSETS))]
    }
}

/// `baudelaire:pages`: the authored content pages as `{ url, label,
/// collection, lang, date, display, note, description, taxonomies, extra }`,
/// for client nav, related-posts, and prefetch.
///
/// One row is [`Page::entry`], the same value a generated listing's entries and
/// the Typst `@baudelaire/pages` catalogue are built from, so the shape a theme
/// learns once holds everywhere.
pub(super) struct Pages;

impl Module for Pages {
    fn entries(&self, cx: &ModuleCx) -> Vec<(String, String)> {
        let pages = Page::catalogue(cx.pages, cx.config).into_values().flatten();
        vec![("baudelaire:pages".into(), Esm::value(&Value::array(pages)))]
    }

    fn types(&self, _cx: &ModuleCx) -> Vec<(String, Dts)> {
        // Declared rather than read off the pages, so the type is the same on a
        // site with no content as on a full one. The keys are
        // `content::listing::Item::value`'s, which a test pins against it.
        vec![("baudelaire:pages".into(), Dts::new().part(PAGES))]
    }
}

/// `baudelaire:sections`: the site's section trees keyed by language code
/// (`sections.fr`), each `{ id, pages: [{ url, title }], children: [...] }` per
/// content directory, nested, exactly what a page of that language gets as
/// `page.sections`, for building menus and command palettes client-side.
pub(super) struct Sections;

impl Module for Sections {
    fn entries(&self, cx: &ModuleCx) -> Vec<(String, String)> {
        // Reuse the tree already built for `page.sections`.
        vec![("baudelaire:sections".into(), Esm::value(cx.sections))]
    }

    fn types(&self, _cx: &ModuleCx) -> Vec<(String, Dts)> {
        vec![("baudelaire:sections".into(), Dts::new().part(SECTIONS))]
    }
}

/// `baudelaire:taxonomies`: each taxonomy's terms mapped to the pages that carry
/// them: `{ tags: { rust: [{ url, title }], .. }, .. }`, for client-side tag
/// filtering and term clouds.
pub(super) struct Taxonomies;

impl Module for Taxonomies {
    fn entries(&self, cx: &ModuleCx) -> Vec<(String, String)> {
        // taxonomy -> term -> pages, sorted for deterministic output.
        let mut taxos: BTreeMap<&str, BTreeMap<&str, Vec<Value>>> = BTreeMap::new();
        for page in cx
            .pages
            .iter()
            .filter(|p| !matches!(p.data, Data::Generated { .. }))
        {
            let link = Value::dict([
                ("url", Value::str(&page.permalink)),
                ("title", Value::str(page.title())),
                ("lang", Value::str(&page.lang)),
            ]);
            for (taxonomy, terms) in &page.frontmatter.taxonomies {
                let by_term = taxos.entry(taxonomy).or_default();
                for term in terms {
                    by_term.entry(term).or_default().push(link.clone());
                }
            }
        }
        let data = Value::dict(taxos.into_iter().map(|(taxonomy, terms)| {
            let terms = Value::dict(
                terms
                    .into_iter()
                    .map(|(term, links)| (term, Value::Array(links))),
            );
            (taxonomy, terms)
        }));
        vec![("baudelaire:taxonomies".into(), Esm::value(&data))]
    }

    fn types(&self, _cx: &ModuleCx) -> Vec<(String, Dts)> {
        vec![("baudelaire:taxonomies".into(), Dts::new().part(TAXONOMIES))]
    }
}

/// `baudelaire:feed`: the most recent dated pages as `{ url, title, date }`,
/// newest first, capped at the feed's configured `limit`, for a "latest posts"
/// widget without fetching a feed file.
pub(super) struct Feed;

impl Module for Feed {
    fn entries(&self, cx: &ModuleCx) -> Vec<(String, String)> {
        // Every language's recent pages, each tagged, rather than only the
        // default language's: one bundle serves the whole site, so a French
        // page's widget had nothing of its own to show.
        let items = cx.config.langs().into_iter().flat_map(|lang| {
            Page::recent(cx.pages, cx.config, lang, cx.config.generate.feed.limit)
                .into_iter()
                .map(|page| {
                    Value::dict([
                        ("url", Value::str(&page.permalink)),
                        ("title", Value::str(page.title())),
                        ("lang", Value::str(&page.lang)),
                        (
                            "date",
                            Value::opt(page.frontmatter.date.map(|d| Iso(d).to_string())),
                        ),
                    ])
                })
                .collect::<Vec<_>>()
        });
        vec![("baudelaire:feed".into(), Esm::value(&Value::array(items)))]
    }

    fn types(&self, _cx: &ModuleCx) -> Vec<(String, Dts)> {
        vec![("baudelaire:feed".into(), Dts::new().part(FEED))]
    }
}

/// `baudelaire:i18n`: the declared `languages` (`{ code, name }`, default first)
/// and their UI-string tables keyed by code (`strings.fr.more`), for a
/// client-side language switcher and localized UI text.
pub(super) struct I18n;

impl I18n {
    /// The declared languages and their string tables, the value the module
    /// exports and the value its declaration is typed from.
    fn data(cx: &ModuleCx) -> Value {
        let codes = cx.config.langs();
        let languages = Value::array(codes.iter().map(|code| {
            Value::dict([
                ("code", Value::str(code)),
                ("name", Value::str(cx.config.name(code).unwrap_or(code))),
                ("dir", Value::str(cx.config.dir(code).unwrap_or("ltr"))),
            ])
        }));
        let strings = Value::dict(codes.iter().map(|code| {
            let table = Value::dict(cx.config.strings(code).iter().cloned());
            ((*code).to_owned(), table)
        }));
        Value::dict([("languages", languages), ("strings", strings)])
    }
}

impl Module for I18n {
    fn entries(&self, cx: &ModuleCx) -> Vec<(String, String)> {
        vec![("baudelaire:i18n".into(), Esm::object(&Self::data(cx)))]
    }

    fn types(&self, cx: &ModuleCx) -> Vec<(String, Dts)> {
        vec![("baudelaire:i18n".into(), Esm::typed(&Self::data(cx)))]
    }
}
