//! The `baudelaire:*` modules serving baudelaire's own generated clients: the
//! search palette and the navigation runtime. Their exports are code, not site
//! data, so their declarations are hand-written fragments rather than types
//! read off a value.

use crate::config::{Named, Prefetch, SearchFormat};
use crate::engine::emit::dts::Dts;

use super::{Module, ModuleCx, Names};

/// The declarations for the modules here, embedded rather than built in Rust:
/// they are TypeScript, and a `.d.ts` on disk is checked by an editor like any
/// other.
const SEARCH: &str = include_str!("types/search.d.ts");
const SPA: &str = include_str!("types/spa.d.ts");

/// `baudelaire:search` (plus `/json`, `/inverted`): baudelaire's generated
/// search-palette client, so a user's entry can mount it and have it bundled.
pub(super) struct Search;

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
pub(super) struct Navigation;

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
