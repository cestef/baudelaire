//! Virtual JS modules: the `baudelaire:*` specifiers a user's bundle can import
//! and have inlined, tree-shaken, minified, and fingerprinted like first-party
//! code. Each [`Module`] generates ES-module source from the site's build data;
//! [`Virtual`] is the single rolldown plugin that serves them all, assembled
//! from the registry in [`builtin`]. Adding a module is one impl and one line.

use std::borrow::Cow;
use std::collections::{BTreeMap, HashMap};
use std::fmt::Write as _;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use rolldown::plugin::{
    HookLoadArgs, HookLoadOutput, HookLoadReturn, HookResolveIdArgs, HookResolveIdOutput,
    HookResolveIdReturn, HookUsage, Plugin, PluginContext, SharedLoadPluginContext,
};
use rolldown_common::ModuleType;

use crate::codegen::{Js, Value};
use crate::config::{Config, Named, Prefetch, SearchFormat};
use crate::content::{Data, Iso, Page};
use crate::engine::emit::dts::Dts;
use crate::error::Result;
use crate::render::AssetMap;
use crate::world::BuildContext;

/// The hand-written declarations, one per module whose shape is fixed rather
/// than read off the site's own data. Embedded rather than built in Rust: they
/// are TypeScript, and a `.d.ts` on disk is checked by an editor like any other.
const SEARCH: &str = include_str!("types/search.d.ts");
const SPA: &str = include_str!("types/spa.d.ts");
const ASSETS: &str = include_str!("types/assets.d.ts");
const PAGES: &str = include_str!("types/pages.d.ts");
const SECTIONS: &str = include_str!("types/sections.d.ts");
const TAXONOMIES: &str = include_str!("types/taxonomies.d.ts");
const FEED: &str = include_str!("types/feed.d.ts");

/// The read-only build data every virtual module generates from.
pub(super) struct ModuleCx<'a> {
    pub config: &'a Config,
    pub pages: &'a [Page],
    pub assets: &'a AssetMap,
    /// The `sys.inputs.baudelaire` value: `baudelaire:site` / `:config` serve
    /// its sub-trees, so Typst and JS read one build context.
    pub context: &'a Value,
    /// The section tree value, already built for `page.sections`.
    pub sections: &'a Value,
}

/// One provider of `baudelaire:*` virtual modules.
trait Module {
    /// The `specifier -> ES-module source` pairs this module serves. Most return
    /// one; [`Search`] serves a family (bare plus per-format).
    fn entries(&self, cx: &ModuleCx) -> Vec<(String, String)>;

    /// The `specifier -> declaration` pairs typing what [`entries`] serves.
    /// Nothing on disk holds these modules, so a TypeScript entry importing one
    /// reads it as unknown until [`Declarations`] writes these out.
    ///
    /// No default: a module that served JavaScript and declared nothing would
    /// be an import a typed bundle cannot use, and the gap would be silent.
    ///
    /// [`entries`]: Module::entries
    fn types(&self, cx: &ModuleCx) -> Vec<(String, Dts)>;
}

/// The registered virtual modules. Adding one is an impl plus a line here, so
/// the list is a `Vec` rather than a fixed-size array whose length is a second
/// place to edit.
fn builtin() -> Vec<Box<dyn Module>> {
    vec![
        Box::new(Search),
        Box::new(Navigation),
        Box::new(Site),
        Box::new(Settings),
        Box::new(Assets),
        Box::new(Pages),
        Box::new(Sections),
        Box::new(Taxonomies),
        Box::new(Feed),
        Box::new(I18n),
    ]
}

/// The one rolldown plugin serving every virtual module: a flat `specifier ->
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

/// The TypeScript declarations for every `baudelaire:*` module, as one ambient
/// `.d.ts` a project can put on its `tsconfig.json` `include` list.
///
/// The counterpart of [`crate::world::Packages`] for the JavaScript side, and
/// written for the same reason: the modules are generated during a build, so an
/// editor has nothing on disk to resolve and reads every import of one as
/// unknown. A build never reads this file, so a stale copy cannot change a page.
pub struct Declarations {
    source: String,
}

impl Declarations {
    /// The header the file opens with: it is generated state, and a reader who
    /// finds it has to know both where it came from and what to do with it.
    const HEADER: &'static str = "// Generated by `baudelaire packages`. Rewritten on every run; edits are lost.\n\
         // Put this file on your `tsconfig.json` `include` list to type the\n\
         // `baudelaire:*` modules a bundled entry imports.\n";

    /// Where the file is written, relative to the project root.
    ///
    /// Under [`Config::SCRATCH`] with the other generated tables: it is
    /// regenerable, gitignored, and outside every root `serve` watches, so
    /// writing it cannot retrigger a build.
    pub fn path() -> PathBuf {
        Config::scratch("generated").join("baudelaire.d.ts")
    }

    /// The declarations this project's modules would serve. Data-shaped modules
    /// (`site`, `config`, `i18n`) are typed from the site's own values, so a
    /// project that has never been built still gets its own keys: they come
    /// from the config, not from the page set.
    pub fn new(config: &Config) -> Self {
        let context = Value::from(&BuildContext::of(config));
        // No pages and no assets: nothing here is typed from a page, and the
        // asset map's *type* is the same however many entries it holds.
        let cx = ModuleCx {
            config,
            pages: &[],
            assets: &AssetMap::new(config.asset_prefix()),
            context: &context,
            sections: &Value::dict::<&str>([]),
        };
        Self::of(&cx)
    }

    /// The declarations for one build context, gathered from every registered
    /// module and sorted by specifier so the file is stable across runs.
    fn of(cx: &ModuleCx) -> Self {
        let mut modules: Vec<(String, Dts)> = builtin()
            .iter()
            .flat_map(|module| module.types(cx))
            .collect();
        modules.sort_by(|(a, _), (b, _)| a.cmp(b));
        let source = std::iter::once(Self::HEADER.to_owned())
            .chain(modules.iter().map(|(specifier, dts)| dts.module(specifier)))
            .collect::<Vec<_>>()
            .join("\n");
        Self { source }
    }

    /// Write the file under `root`, creating its directory.
    pub fn write(&self, root: &Path) -> Result<()> {
        crate::fs::write_all(root.join(Self::path()), &self.source)
    }

    /// Remove what a previous run wrote under `root`. `false` when there was
    /// nothing to remove.
    pub fn remove(root: &Path) -> Result<bool> {
        let path = root.join(Self::path());
        if !path.exists() {
            return Ok(false);
        }
        crate::fs::remove_file(&path)?;
        Ok(true)
    }
}

/// Displays a [`Named`] enum's spellings as a TypeScript string union, so a
/// declaration offers exactly the names the config parses.
struct Names<T: Named + 'static>(&'static [(&'static str, T)]);

impl<T: Named> std::fmt::Display for Names<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for (i, (name, _)) in self.0.iter().enumerate() {
            if i > 0 {
                f.write_str(" | ")?;
            }
            write!(f, "\"{name}\"")?;
        }
        Ok(())
    }
}

/// ES-module source builders, shared by the data modules so they emit exports
/// one way: a default export always, plus a named const per top-level object
/// key that is a safe identifier (so `import { title }` tree-shakes).
struct Esm;

impl Esm {
    /// The declaration matching [`Esm::object`]: the same named consts and the
    /// same default, typed from the value itself, so a site's own keys are what
    /// an editor completes.
    fn typed(value: &Value) -> Dts {
        let mut dts = Dts::new();
        if let Value::Dict(pairs) = value {
            for (key, item) in pairs {
                if Self::ident(key) {
                    dts = dts.constant(key, item);
                }
            }
        }
        dts.default(value)
    }

    /// A module exporting `value` as default and each valid-identifier key of it
    /// (when it is a dict) as a named const.
    fn object(value: &Value) -> String {
        let mut out = String::new();
        if let Value::Dict(pairs) = value {
            for (key, item) in pairs {
                if Self::ident(key) {
                    let _ = writeln!(out, "export const {key} = {};", Js(item));
                }
            }
        }
        let _ = writeln!(out, "export default {};", Js(value));
        out
    }

    /// A module with a single default export of `value`.
    fn value(value: &Value) -> String {
        format!("export default {};\n", Js(value))
    }

    /// Whether `s` is a safe, non-reserved JS identifier for a named export.
    /// The spelling of an identifier is [`codegen::ident`]; a reserved word is
    /// a legal object key but cannot be bound, which is this module's concern
    /// and not the renderer's.
    ///
    /// [`codegen::ident`]: crate::codegen::ident
    fn ident(s: &str) -> bool {
        const RESERVED: &[&str] = &[
            "default", "class", "const", "let", "var", "function", "return", "import", "export",
            "new", "delete", "typeof", "in", "of", "do", "if", "else", "switch", "case", "for",
            "while", "break", "continue", "this", "super", "void", "yield", "await", "null",
            "true", "false",
        ];
        crate::codegen::ident(s) && !RESERVED.contains(&s)
    }
}

/// `baudelaire:search` (plus `/json`, `/inverted`): baudelaire's generated
/// search-palette client, so a user's entry can mount it and have it bundled.
struct Search;

impl Search {
    /// The bare specifier, which follows whichever index the build emits, and
    /// the two that pin a format. Spelled once, since both the sources and the
    /// declarations are keyed by them.
    const BARE: &'static str = "baudelaire:search";
    const JSON: &'static str = "baudelaire:search/json";
    const INVERTED: &'static str = "baudelaire:search/inverted";

    /// The format the bare specifier serves: inverted only when that is the
    /// sole configured format, else the flat client (it has snippets).
    fn default(cx: &ModuleCx) -> SearchFormat {
        match cx.config.generate.search.formats == [SearchFormat::Inverted] {
            true => SearchFormat::Inverted,
            false => SearchFormat::Json,
        }
    }
}

impl Module for Search {
    fn entries(&self, cx: &ModuleCx) -> Vec<(String, String)> {
        let base = cx.config.base_path();
        // The bundled client is site-wide, so it defaults to the default
        // language's index; `createSearch(url)` takes another language's.
        let lang = &cx.config.lang;
        let module = |format: SearchFormat| format.module(base, &format.index(cx.config, lang));
        vec![
            (Self::BARE.into(), module(Self::default(cx))),
            (Self::JSON.into(), module(SearchFormat::Json)),
            (Self::INVERTED.into(), module(SearchFormat::Inverted)),
        ]
    }

    fn types(&self, _cx: &ModuleCx) -> Vec<(String, Dts)> {
        // The format-pinned specifiers re-export the bare one's declaration:
        // the formats differ in how they rank, not in what they hand back.
        vec![
            (Self::BARE.to_owned(), Dts::new().part(SEARCH)),
            (Self::JSON.to_owned(), Dts::new().same_as(Self::BARE)),
            (Self::INVERTED.to_owned(), Dts::new().same_as(Self::BARE)),
        ]
    }
}

/// `baudelaire:spa`: the client-side navigation runtime, so a site bundling its
/// own entry can mount it (and pick its own container) instead of loading the
/// generated `spa.js` separately. Served whether or not `navigation { spa { } }` is
/// set: importing it is itself the opt-in, and the block's fields are only the
/// defaults `mountSpa()` starts from.
struct Navigation;

impl Module for Navigation {
    fn entries(&self, cx: &ModuleCx) -> Vec<(String, String)> {
        vec![("baudelaire:spa".into(), cx.config.navigation.spa.module())]
    }

    fn types(&self, _cx: &ModuleCx) -> Vec<(String, Dts)> {
        // The prefetch policies come from the config enum that parses them, so
        // the declaration cannot offer a spelling the runtime does not know.
        let dts = Dts::new()
            .alias("Prefetch", Names(Prefetch::NAMES))
            .part(SPA);
        vec![("baudelaire:spa".into(), dts)]
    }
}

/// `baudelaire:site`: site identity and build version, mirroring what templates
/// read from `sys.inputs.baudelaire`.
///
/// The field list belongs to [`BuildContext::site_fields`], which the Typst
/// module `@baudelaire/site` serves too, so the two cannot drift.
struct Site;

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
struct Settings;

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

    fn types(&self, _cx: &ModuleCx) -> Vec<(String, Dts)> {
        vec![("baudelaire:assets".into(), Dts::new().part(ASSETS))]
    }
}

/// `baudelaire:pages`: the authored content pages as `{ url, label,
/// collection, lang, date, display, note, taxonomies, extra }`, for client
/// nav, related-posts, and prefetch.
///
/// One row is [`Page::entry`], the same value a generated listing's entries and
/// the Typst `@baudelaire/pages` catalogue are built from, so the shape a theme
/// learns once holds everywhere.
struct Pages;

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
struct Sections;

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
struct Taxonomies;

impl Module for Taxonomies {
    fn entries(&self, cx: &ModuleCx) -> Vec<(String, String)> {
        // taxonomy -> term -> pages, sorted for deterministic output.
        let mut taxos: BTreeMap<&str, BTreeMap<&str, Vec<Value>>> = BTreeMap::new();
        for page in cx
            .pages
            .iter()
            .filter(|p| !matches!(p.data, Data::Generated(_)))
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
struct Feed;

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
struct I18n;

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

#[cfg(test)]
mod tests {
    use super::{Declarations, PAGES};
    use crate::codegen::Value;
    use crate::config::Config;
    use crate::content::listing::Item;

    /// Every specifier the bundler serves has to be declared, or a TypeScript
    /// entry importing it reads as unknown while the build resolves it fine.
    #[test]
    fn every_served_module_is_declared() {
        let config = Config::default();
        let source = Declarations::new(&config).source;
        let context = Value::from(&crate::world::BuildContext::of(&config));
        let cx = super::ModuleCx {
            config: &config,
            pages: &[],
            assets: &crate::render::AssetMap::new(config.asset_prefix()),
            context: &context,
            sections: &Value::dict::<&str>([]),
        };
        for module in super::builtin() {
            for (specifier, _) in module.entries(&cx) {
                assert!(
                    source.contains(&format!("declare module \"{specifier}\"")),
                    "{specifier} is served but not declared:\n{source}"
                );
            }
        }
    }

    /// The catalogue row is declared by hand, so nothing but this stops it from
    /// drifting away from the value the module actually serves.
    #[test]
    fn the_declared_row_names_every_field_a_row_carries() {
        let Value::Dict(fields) = Item::new("/a/", "A").value() else {
            panic!("a row is a dict");
        };
        for (key, _) in fields {
            assert!(
                PAGES.contains(&format!("{key}:")),
                "`{key}` is in a catalogue row and not in its declaration"
            );
        }
    }

    /// A site's own `client { }` keys are what an editor completes, which is
    /// the whole reason the data modules are typed from their values.
    #[test]
    fn the_client_block_is_typed_from_its_own_keys() {
        let config = Config::load(
            "site \"T\"\nclient {\n  api \"https://example.com\"\n  retries 3\n}\n",
            std::path::Path::new("."),
        )
        .expect("config");
        let source = Declarations::new(&config).source;

        assert!(source.contains("export const api: string;"), "{source}");
        assert!(source.contains("export const retries: number;"), "{source}");
    }
}
