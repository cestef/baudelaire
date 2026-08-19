//! Client-side search indexes: one [`Corpus`] built from every page's rendered
//! HTML, serialized into each configured [`SearchFormat`].
//!
//! - [`SearchFormat::Json`] -> `search.json`: a flat array of documents
//!   `[{ "url", "title", "tags", "body" }]`.
//! - [`SearchFormat::Inverted`] -> `search.inverted.json`: a prebuilt index
//!   `{ "documents": [{ "url", "title" }], "postings": { term: [docId..] } }`.

mod client;
mod corpus;
mod index;

use std::path::PathBuf;

use corpus::Corpus;

use super::{Emit, Processor, Site};
use crate::config::Config;
use crate::error::Result;

/// Emits every configured search index format from one shared corpus.
pub(super) struct SearchIndex;

impl Processor for SearchIndex {
    fn name(&self) -> &'static str {
        "the search index"
    }

    fn claims(&self, config: &Config) -> Vec<PathBuf> {
        let cfg = &config.generate.search;
        let mut out = Vec::new();
        for lang in config.langs() {
            let scope = config.scope(lang, "");
            for &format in &cfg.formats {
                out.push(Site::at(config, &[&scope, format.file()]));
                if cfg.ui {
                    out.push(Site::at(config, &[&scope, format.client_file()]));
                }
            }
        }
        out
    }

    fn enabled(&self, config: &Config) -> bool {
        !config.generate.search.formats.is_empty()
    }

    fn run(&self, site: &Site, out: &mut dyn Emit) -> Result<()> {
        let cfg = &site.config.generate.search;
        for lang in site.config.langs() {
            let scope = site.config.scope(lang, "");
            let corpus = Corpus::build(site, cfg, lang);
            for &format in &cfg.formats {
                let path = site.dist(&[&scope, format.file()]);
                out.file(&path, &corpus.json(format, cfg)?)?;
                out.wrote_with(&path, format_args!("{} docs", corpus.len()));
                if cfg.ui {
                    let path = site.dist(&[&scope, format.client_file()]);
                    out.file(
                        &path,
                        &format.client(site.config.base_path(), &format.index(site.config, lang)),
                    )?;
                    out.wrote(&path);
                }
            }
        }
        Ok(())
    }
}
