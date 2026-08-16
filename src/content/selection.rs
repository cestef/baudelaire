//! Which pages, in what order, under what title: the one question everything
//! that binds many pages into a single artifact asks.

use crate::config::{BundleConfig, Config, SortKey};

use super::Page;

/// The pages one artifact binds, in order, and what the whole is called.
pub struct Selection<'a> {
    /// The id the artifact is written and cached under: the bundle's own id,
    /// suffixed with the language on a multilingual site.
    pub id: String,
    pub title: String,
    /// The language every bound page is in.
    pub lang: &'a str,
    pub pages: Vec<&'a Page>,
}

impl<'a> Selection<'a> {
    /// Every selection a bundle asks for: one per built language, in the order
    /// the config names them. A language binding no page yields nothing at all,
    /// rather than an empty document.
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

    /// Put the bound pages in order: the bundle's own `sort` when it states
    /// one, a single bound collection's own key when it does not, and otherwise
    /// the order the pages arrived in, since re-sorting a multi-collection
    /// selection by one key would interleave collections that never mix.
    fn order(bound: &mut [&'a Page], cfg: &BundleConfig, config: &Config) {
        if let Some(sort) = cfg.sort {
            bound.sort_by(|a, b| Page::compare(sort, a, b));
        } else if !cfg.site && cfg.collections.len() == 1 {
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
            return cfg.collections[0].clone();
        }
        config.title(lang).to_owned()
    }

    /// A selection's id: the bundle's, plus the language on a site that builds
    /// more than one, so two editions never claim one file.
    fn named(id: &str, lang: &str, config: &Config) -> String {
        if config.langs().len() > 1 {
            format!("{id}.{lang}")
        } else {
            id.to_owned()
        }
    }

    /// Where a file of this selection is served: `/<id>.<ext>`, localized like
    /// every other per-language artifact.
    pub fn url(&self, config: &Config, ext: &str) -> String {
        format!("/{}.{ext}", config.scope(self.lang, self.stem()))
    }

    /// The id without the language suffix `named` may have added, since the URL
    /// scope already puts the language in the path.
    fn stem(&self) -> &str {
        self.id
            .strip_suffix(&format!(".{}", self.lang))
            .unwrap_or(&self.id)
    }
}
