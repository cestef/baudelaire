//! Virtual JS modules: the `baudelaire:*` specifiers a user's bundle can import
//! and have inlined, tree-shaken, minified, and fingerprinted like first-party
//! code. Each [`Module`] generates ES-module source from the site's build data;
//! [`Virtual`] is the single rolldown plugin that serves them all, assembled
//! from the registry in [`builtin`]. Adding a module is one impl and one line.

use std::borrow::Cow;
use std::collections::{BTreeMap, HashMap};
use std::future::Future;
use std::sync::Arc;

use rolldown::plugin::{
    HookLoadArgs, HookLoadOutput, HookLoadReturn, HookResolveIdArgs, HookResolveIdOutput,
    HookResolveIdReturn, HookUsage, Plugin, PluginContext, SharedLoadPluginContext,
};
use rolldown_common::ModuleType;

use crate::codegen::{Js, Value};
use crate::config::{Config, SearchFormat};
use crate::content::{Data, Page, Section};
use crate::render::AssetMap;

/// The read-only build data every virtual module generates from: the config, the
/// planned pages, and the finalized asset URL map.
pub(super) struct ModuleCx<'a> {
    pub config: &'a Config,
    pub pages: &'a [Page],
    pub assets: &'a AssetMap,
}

/// One provider of `baudelaire:*` virtual modules.
trait Module {
    /// The `specifier → ES-module source` pairs this module serves. Most return
    /// one; [`Search`] serves a family (bare plus per-format).
    fn entries(&self, cx: &ModuleCx) -> Vec<(String, String)>;
}

/// The registered virtual modules.
fn builtin() -> [Box<dyn Module>; 8] {
    [
        Box::new(Search),
        Box::new(Site),
        Box::new(Settings),
        Box::new(Assets),
        Box::new(Pages),
        Box::new(Sections),
        Box::new(Taxonomies),
        Box::new(Feed),
    ]
}

/// The one rolldown plugin serving every virtual module: a flat `specifier →
/// source` table built once from [`builtin`] against the site context.
#[derive(Debug)]
pub(super) struct Virtual {
    modules: HashMap<String, Arc<str>>,
}

impl Virtual {
    pub(super) fn new(cx: &ModuleCx) -> Self {
        let modules = builtin()
            .iter()
            .flat_map(|module| module.entries(cx))
            .map(|(id, src)| (id, Arc::from(src.as_str())))
            .collect();
        Self { modules }
    }
}

impl Plugin for Virtual {
    fn name(&self) -> Cow<'static, str> {
        "baudelaire:virtual".into()
    }

    fn resolve_id(
        &self,
        _ctx: &PluginContext,
        args: &HookResolveIdArgs<'_>,
    ) -> impl Future<Output = HookResolveIdReturn> + Send {
        // Claim our specifiers so rolldown skips filesystem resolution.
        let resolved = self
            .modules
            .contains_key(args.specifier)
            .then(|| HookResolveIdOutput::from_id(args.specifier));
        async move { Ok(resolved) }
    }

    fn load(
        &self,
        _ctx: SharedLoadPluginContext,
        args: &HookLoadArgs<'_>,
    ) -> impl Future<Output = HookLoadReturn> + Send {
        let loaded = self.modules.get(args.id).map(|code| HookLoadOutput {
            code: code.to_string().into(),
            module_type: Some(ModuleType::Js),
            ..HookLoadOutput::default()
        });
        async move { Ok(loaded) }
    }

    fn register_hook_usage(&self) -> HookUsage {
        HookUsage::ResolveId | HookUsage::Load
    }
}

/// ES-module source builders, shared by the data modules so they emit exports
/// one way: a default export always, plus a named const per top-level object
/// key that is a safe identifier (so `import { title }` tree-shakes).
struct Esm;

impl Esm {
    /// A module exporting `value` as default and each valid-identifier key of it
    /// (when it is a dict) as a named const.
    fn object(value: &Value) -> String {
        let mut out = String::new();
        if let Value::Dict(pairs) = value {
            for (key, item) in pairs {
                if Self::ident(key) {
                    out.push_str(&format!("export const {key} = {};\n", Js(item)));
                }
            }
        }
        out.push_str(&format!("export default {};\n", Js(value)));
        out
    }

    /// A module with a single default export of `value`.
    fn value(value: &Value) -> String {
        format!("export default {};\n", Js(value))
    }

    /// Whether `s` is a safe, non-reserved JS identifier for a named export.
    fn ident(s: &str) -> bool {
        const RESERVED: &[&str] = &[
            "default", "class", "const", "let", "var", "function", "return", "import", "export",
            "new", "delete", "typeof", "in", "of", "do", "if", "else", "switch", "case", "for",
            "while", "break", "continue", "this", "super", "void", "yield", "await", "null",
            "true", "false",
        ];
        let mut chars = s.chars();
        let head =
            matches!(chars.next(), Some(c) if c.is_ascii_alphabetic() || c == '_' || c == '$');
        head && chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$')
            && !RESERVED.contains(&s)
    }
}

/// `baudelaire:search` (plus `/json`, `/inverted`): baudelaire's generated
/// search-palette client, so a user's entry can mount it and have it bundled.
struct Search;

impl Module for Search {
    fn entries(&self, cx: &ModuleCx) -> Vec<(String, String)> {
        // The bare specifier follows the emitted index: inverted only when that
        // is the sole configured format, else the flat client (it has snippets).
        let default = if cx.config.search.formats == [SearchFormat::Inverted] {
            SearchFormat::Inverted
        } else {
            SearchFormat::Json
        };
        vec![
            ("baudelaire:search".into(), default.module()),
            ("baudelaire:search/json".into(), SearchFormat::Json.module()),
            (
                "baudelaire:search/inverted".into(),
                SearchFormat::Inverted.module(),
            ),
        ]
    }
}

/// `baudelaire:site`: site identity and build version, mirroring what templates
/// read from `sys.inputs.baudelaire`.
struct Site;

impl Module for Site {
    fn entries(&self, cx: &ModuleCx) -> Vec<(String, String)> {
        let c = cx.config;
        let data = Value::dict([
            ("title", Value::opt(c.site.clone())),
            ("url", Value::opt(c.url.clone())),
            ("lang", Value::str(&c.lang)),
            ("author", Value::opt(c.author.clone())),
            ("version", Value::str(env!("CARGO_PKG_VERSION"))),
        ]);
        vec![("baudelaire:site".into(), Esm::object(&data))]
    }
}

/// `baudelaire:config`: user-defined build-time constants from the `client { }`
/// config block — baudelaire's answer to Vite's `define`.
struct Settings;

impl Module for Settings {
    fn entries(&self, cx: &ModuleCx) -> Vec<(String, String)> {
        let data = Value::dict(cx.config.client.iter().cloned());
        vec![("baudelaire:config".into(), Esm::object(&data))]
    }
}

/// `baudelaire:assets`: the request → fingerprinted-URL map, with a `url()`
/// lookup that falls back to the input — so client JS can reference a hashed
/// asset by its logical path, the way CSS and HTML already do.
struct Assets;

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
}

/// `baudelaire:pages`: the authored content pages as `{ url, title, collection,
/// date, taxonomies }`, for client nav, related-posts, and prefetch.
struct Pages;

impl Module for Pages {
    fn entries(&self, cx: &ModuleCx) -> Vec<(String, String)> {
        let pages = cx
            .pages
            .iter()
            .filter(|p| !matches!(p.data, Data::Generated(_)))
            .map(|page| {
                let taxonomies =
                    Value::dict(page.frontmatter.taxonomies.iter().map(|(key, terms)| {
                        (key.clone(), Value::array(terms.iter().map(Value::str)))
                    }));
                Value::dict([
                    ("url", Value::str(&page.permalink)),
                    ("title", Value::str(page.title())),
                    ("collection", Value::str(&page.collection)),
                    ("date", Value::opt(page.frontmatter.date.map(Iso::date))),
                    ("taxonomies", taxonomies),
                ])
            });
        vec![("baudelaire:pages".into(), Esm::value(&Value::array(pages)))]
    }
}

/// `baudelaire:sections`: the site's section tree — `{ id, pages: [{ url, title
/// }], children: [...] }` per content directory, nested — exactly what templates
/// get as `page.sections`, for building menus and command palettes client-side.
struct Sections;

impl Module for Sections {
    fn entries(&self, cx: &ModuleCx) -> Vec<(String, String)> {
        let tree = Section::tree(cx.pages, cx.config);
        let data = Value::array(tree.iter().map(|s| s.value()));
        vec![("baudelaire:sections".into(), Esm::value(&data))]
    }
}

/// `baudelaire:taxonomies`: each taxonomy's terms mapped to the pages that carry
/// them — `{ tags: { rust: [{ url, title }], .. }, .. }` — for client-side tag
/// filtering and term clouds.
struct Taxonomies;

impl Module for Taxonomies {
    fn entries(&self, cx: &ModuleCx) -> Vec<(String, String)> {
        // taxonomy → term → pages, sorted for deterministic output.
        let mut taxos: BTreeMap<&str, BTreeMap<&str, Vec<Value>>> = BTreeMap::new();
        for page in cx
            .pages
            .iter()
            .filter(|p| !matches!(p.data, Data::Generated(_)))
        {
            let link = Value::dict([
                ("url", Value::str(&page.permalink)),
                ("title", Value::str(page.title())),
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
}

/// `baudelaire:feed`: the most recent dated pages as `{ url, title, date }`,
/// newest first, capped at the feed's configured `limit` — for a "latest posts"
/// widget without fetching a feed file.
struct Feed;

impl Module for Feed {
    fn entries(&self, cx: &ModuleCx) -> Vec<(String, String)> {
        let mut dated: Vec<&Page> = cx
            .pages
            .iter()
            .filter(|p| !matches!(p.data, Data::Generated(_)) && p.frontmatter.date.is_some())
            .collect();
        dated.sort_by_key(|p| std::cmp::Reverse(p.frontmatter.date));
        let items = dated.into_iter().take(cx.config.feed.limit).map(|page| {
            Value::dict([
                ("url", Value::str(&page.permalink)),
                ("title", Value::str(page.title())),
                ("date", Value::opt(page.frontmatter.date.map(Iso::date))),
            ])
        });
        vec![("baudelaire:feed".into(), Esm::value(&Value::array(items)))]
    }
}

/// ISO-8601 date formatting for the page data modules, matching the display
/// dates listings emit.
struct Iso;

impl Iso {
    fn date(d: time::Date) -> String {
        format!("{:04}-{:02}-{:02}", d.year(), u8::from(d.month()), d.day())
    }
}
