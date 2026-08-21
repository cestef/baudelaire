//! What the page set makes of each page: the siblings it sits between, and its
//! editions in other languages.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::config::Config;

use super::Page;
use super::page::{Siblings, Translation};

/// One page's place among its neighbours.
#[derive(Hash, Debug, Default, Clone)]
pub struct Related {
    pub siblings: Siblings,
    /// Every language this page was written in, its own included. Empty on a
    /// single-language site, and for a page no other language answers to.
    pub translations: Vec<Translation>,
}

impl Related {
    /// A page with no neighbours, which is what an unrelated page reads as.
    const NONE: Self = Self {
        siblings: Siblings {
            prev: None,
            next: None,
        },
        translations: Vec::new(),
    };
}

/// Where every page sits among the others, keyed by the output file it claims,
/// which the plan's collision check has already proven names one page.
///
/// A side table rather than two fields on [`Page`], because neither answer
/// exists until the whole page set does: a page carries what its source says,
/// and the plan carries what the page set says.
#[derive(Hash, Debug, Default)]
pub struct Relations(BTreeMap<PathBuf, Related>);

/// The answer for a page nothing related: a page compiled outside a plan, or a
/// fixture.
static UNRELATED: Related = Related::NONE;

impl Relations {
    /// The empty table, for a caller with no plan behind it: every page in it
    /// stands alone.
    pub fn none() -> &'static Self {
        static NONE: std::sync::LazyLock<Relations> = std::sync::LazyLock::new(Relations::default);
        &NONE
    }

    /// What `page` sits between, and what it reads as elsewhere.
    pub fn of(&self, page: &Page) -> &Related {
        self.0.get(&page.output).unwrap_or(&UNRELATED)
    }

    /// Record the pages `output` sits between, in its collection's sort order.
    pub(super) fn between(&mut self, output: &Path, siblings: Siblings) {
        self.0.entry(output.to_owned()).or_default().siblings = siblings;
    }

    /// Pair every page with its editions in other languages, by the key that
    /// identifies one page across them.
    ///
    /// A page whose key nothing else shares is left alone: `translations` is a
    /// language switcher, and a single edition is not one.
    pub(super) fn translate(&mut self, pages: &[Page], config: &Config) {
        let mut editions: BTreeMap<String, Vec<Translation>> = BTreeMap::new();
        for page in pages {
            editions
                .entry(page.identity())
                .or_default()
                .push(Translation {
                    lang: page.lang.clone(),
                    url: page.permalink.clone(),
                    title: page.title().to_owned(),
                });
        }
        let order = config.langs();
        editions.retain(|_, set| set.len() > 1);
        for set in editions.values_mut() {
            set.sort_by_key(|t| order.iter().position(|l| *l == t.lang));
        }
        for page in pages {
            if let Some(set) = editions.get(&page.identity()) {
                self.0
                    .entry(page.output.clone())
                    .or_default()
                    .translations
                    .clone_from(set);
            }
        }
    }
}
