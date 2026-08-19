//! The `baudelaire:*` modules serving baudelaire's own generated clients: the
//! search palette and the navigation runtime.

use crate::config::{Named, Prefetch};
use crate::engine::emit::dts::Dts;

use super::{Module, ModuleCx, Names};

/// The declarations for the modules here, written as TypeScript so an editor
/// checks them like any other `.d.ts`.
const SEARCH: &str = include_str!("types/search.d.ts");
const SPA: &str = include_str!("types/spa.d.ts");

/// `baudelaire:search`: baudelaire's generated search-palette client, so a
/// user's entry can mount it and have it bundled. One specifier, because one
/// client reads either index shape and every language.
pub(super) struct Search;

impl Search {
    const SPECIFIER: &'static str = "baudelaire:search";
}

impl Module for Search {
    fn entries(&self, cx: &ModuleCx) -> Vec<(String, String)> {
        vec![(
            Self::SPECIFIER.into(),
            crate::engine::emit::search::Client::module(cx.config),
        )]
    }

    fn types(&self, _cx: &ModuleCx) -> Vec<(String, Dts)> {
        vec![(Self::SPECIFIER.to_owned(), Dts::new().part(SEARCH))]
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
