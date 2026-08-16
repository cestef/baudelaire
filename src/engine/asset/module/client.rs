//! The `baudelaire:*` modules serving baudelaire's own generated clients: the
//! search palette and the navigation runtime.

use crate::config::{Named, Prefetch, SearchFormat};
use crate::engine::emit::dts::Dts;

use super::{Module, ModuleCx, Names};

/// The declarations for the modules here, written as TypeScript so an editor
/// checks them like any other `.d.ts`.
const SEARCH: &str = include_str!("types/search.d.ts");
const SPA: &str = include_str!("types/spa.d.ts");

/// `baudelaire:search` (plus `/json`, `/inverted`): baudelaire's generated
/// search-palette client, so a user's entry can mount it and have it bundled.
pub(super) struct Search;

impl Search {
    /// The bare specifier, which follows whichever index the build emits, and
    /// the two that pin a format.
    const BARE: &'static str = "baudelaire:search";
    const JSON: &'static str = "baudelaire:search/json";
    const INVERTED: &'static str = "baudelaire:search/inverted";

    /// The format the bare specifier serves: inverted only when that is the
    /// sole configured format, else the flat client (it has snippets).
    fn default(cx: &ModuleCx) -> SearchFormat {
        if cx.config.generate.search.formats == [SearchFormat::Inverted] {
            SearchFormat::Inverted
        } else {
            SearchFormat::Json
        }
    }
}

impl Module for Search {
    fn entries(&self, cx: &ModuleCx) -> Vec<(String, String)> {
        let base = cx.config.base_path();
        let lang = &cx.config.lang;
        let module = |format: SearchFormat| format.module(base, &format.index(cx.config, lang));
        vec![
            (Self::BARE.into(), module(Self::default(cx))),
            (Self::JSON.into(), module(SearchFormat::Json)),
            (Self::INVERTED.into(), module(SearchFormat::Inverted)),
        ]
    }

    fn types(&self, _cx: &ModuleCx) -> Vec<(String, Dts)> {
        vec![
            (Self::BARE.to_owned(), Dts::new().part(SEARCH)),
            (Self::JSON.to_owned(), Dts::new().same_as(Self::BARE)),
            (Self::INVERTED.to_owned(), Dts::new().same_as(Self::BARE)),
        ]
    }
}

/// `baudelaire:spa`: the client-side navigation runtime, so a site bundling its
/// own entry can mount it. Served whether or not `navigation { spa { } }` is
/// set, since importing it is itself the opt-in.
pub(super) struct Navigation;

impl Module for Navigation {
    fn entries(&self, cx: &ModuleCx) -> Vec<(String, String)> {
        vec![("baudelaire:spa".into(), cx.config.navigation.spa.module())]
    }

    fn types(&self, _cx: &ModuleCx) -> Vec<(String, Dts)> {
        let dts = Dts::new()
            .alias("Prefetch", Names(Prefetch::NAMES))
            .part(SPA);
        vec![("baudelaire:spa".into(), dts)]
    }
}
