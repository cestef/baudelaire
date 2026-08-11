//! Which pages, in what order, under what title.
//!
//! One question, asked by everything that binds many pages into one artifact: a
//! typeset PDF, an EPUB, and whatever binds them next. Each used to answer it
//! for itself, and the answers differed in ways nobody chose: one skipped
//! generated listings and another did not, one bound a collection in the site's
//! order and another in the order the config happened to name.
//!
//! The title is part of the selection rather than of the format, because it is
//! the same title whichever file it ends up in: a selection of one collection is
//! that collection's document, and a French edition of it is a French document.

use crate::config::{BundleConfig, Config, SortKey};

use super::Page;

/// The pages one artifact binds, in order, and what the whole is called.
///
/// Resolved from the config and the planned page set before anything compiles,
/// so the prune, the cache and every writer read one list.
pub struct Selection<'a> {
    /// The id the artifact is written under, and the cache names: the bundle's
    /// own id, suffixed with the language on a multilingual site.
    pub id: String,
    /// The document's title, handed to whatever renders it.
    pub title: String,
    /// The language every bound page is in. A French manual is a French
    /// document, and binding both languages into one file would interleave
    /// them.
    pub lang: &'a str,
    pub pages: Vec<&'a Page>,
}

impl<'a> Selection<'a> {
    /// Every selection a bundle asks for: one per built language, in the order
    /// the config names them.
    ///
    /// A language with no page in the selection yields nothing at all, rather
    /// than an empty document: a site that translated its blog and not its
    /// manual would otherwise ship an empty French manual.
    pub fn planned(
        id: &str,
        cfg: &BundleConfig,
        config: &'a Config,
        pages: &'a [Page],
    ) -> Vec<Self> {
        config
            .langs()
            .iter()
            .filter_map(|lang| Self::bind(id, cfg, config, pages, lang))
            .collect()
    }

    /// One language's selection, or `None` when it binds no page.
    fn bind(
        id: &str,
        cfg: &BundleConfig,
        config: &'a Config,
        pages: &'a [Page],
        lang: &'a str,
    ) -> Option<Self> {
        let mut bound: Vec<&'a Page> = pages
            .iter()
            .filter(|page| page.lang == lang)
            // Generated listings are excluded, as they are from every other
            // bound artifact: a tag index inside a manual is a page of links to
            // a document the reader is already holding.
            .filter(|page| page.authored())
            .filter(|page| cfg.site || cfg.collections.iter().any(|id| page.section() == id))
            .collect();
        if bound.is_empty() {
            return None;
        }
        Self::order(&mut bound, cfg, config);
        Some(Self {
            id: Self::named(id, lang, config),
            title: Self::title(cfg, config, lang),
            lang,
            pages: bound,
        })
    }

    /// Put the bound pages in order.
    ///
    /// The bundle's own `sort` when it states one. Otherwise the order the
    /// pages already arrived in, which is each collection's own: they were
    /// sorted when the collection was built, and re-sorting a multi-collection
    /// selection by one key would interleave two collections that were never
    /// meant to mix.
    fn order(bound: &mut [&'a Page], cfg: &BundleConfig, config: &Config) {
        if let Some(sort) = cfg.sort {
            bound.sort_by(|a, b| Page::compare(sort, a, b));
        } else if !cfg.site && cfg.collections.len() == 1 {
            // One collection, no override: its own key, which is what the site
            // shows that collection in and what the pages were already sorted
            // by. Restated here because a selection is built from the whole
            // page set, whose order is the plan's rather than any one
            // collection's.
            let sort = config
                .collection(&cfg.collections[0])
                .map_or_else(SortKey::default, |c| c.sort);
            bound.sort_by(|a, b| Page::compare(sort, a, b));
        }
        if cfg.reverse {
            bound.reverse();
        }
    }

    /// What the document is called: the site's word for it if it gave one, the
    /// bound collection's title when exactly one is bound, and the site's title
    /// otherwise.
    fn title(cfg: &BundleConfig, config: &Config, lang: &str) -> String {
        if let Some(title) = &cfg.title {
            return title.clone();
        }
        if !cfg.site && cfg.collections.len() == 1 {
            // The collection's own id. A collection has no configured title, and
            // inventing one here would be a second spelling of a name the site
            // already has; `title` is what a site says when the id is not the
            // word it wants on the cover.
            return cfg.collections[0].clone();
        }
        config.title(lang).to_owned()
    }

    /// A selection's id: the bundle's, plus the language on a site that builds
    /// more than one, so two editions never claim one cache entry or one file.
    fn named(id: &str, lang: &str, config: &Config) -> String {
        if config.langs().len() > 1 {
            format!("{id}.{lang}")
        } else {
            id.to_owned()
        }
    }

    /// Where a file of this selection is served: `/<id>.<ext>`, localized like
    /// every other per-language artifact. One rule for every format, so a
    /// reader who knows where `/guide.pdf` came from knows where `/guide.epub`
    /// did.
    pub fn url(&self, config: &Config, ext: &str) -> String {
        // The id already carries the language on a multilingual site; the URL
        // scope is what puts it in the path, and both read the same `langs`.
        format!("/{}.{ext}", config.scope(self.lang, self.stem()))
    }

    /// The id without the language suffix `named` may have added: the URL scope
    /// puts the language in the path, and spelling it twice would serve
    /// `/fr/guide.fr.pdf`.
    fn stem(&self) -> &str {
        self.id
            .strip_suffix(&format!(".{}", self.lang))
            .unwrap_or(&self.id)
    }
}
