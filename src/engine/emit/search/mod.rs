//! Client-side search: one [`Corpus`] per language, written as `search.json`,
//! and the one generated palette client that reads whichever of them the page
//! it mounts on belongs to.
//!
//! The two shapes of `generate { search { index } }` differ only in who builds
//! the postings, never in how a query is tokenized, ranked or snippeted:
//!
//! - `terms`: `{ "terms": [..], "postings": [[[doc, score], ..], ..] }`, built
//!   here, so only each hit's snippet ships.
//! - `documents`: each document's prose ships whole and the client indexes it
//!   on load, which is the shape any other client library can read.

mod client;
mod corpus;
mod index;
mod tokens;

use std::path::PathBuf;

pub(crate) use client::Client;
use corpus::Corpus;

use super::{Emit, Processor, Reads, Site};
use crate::config::Config;
use crate::error::Result;

/// Emits one search index per language, and the client that queries them.
pub(super) struct Search;

impl Processor for Search {
    fn name(&self) -> &'static str {
        "the search index"
    }

    fn claims(&self, config: &Config) -> Vec<PathBuf> {
        Emitted::plan(config)
            .into_iter()
            .map(|emitted| emitted.path(config))
            .collect()
    }

    fn inputs(&self, _config: &Config) -> Option<&'static [Reads]> {
        Some(&[Reads::Listing, Reads::Markup])
    }

    fn enabled(&self, config: &Config) -> bool {
        config.generate.search.enabled
    }

    fn run(&self, site: &Site, out: &mut dyn Emit) -> Result<()> {
        for emitted in Emitted::plan(site.config) {
            let path = emitted.path(site.config);
            match emitted {
                Emitted::Index(lang) => {
                    let corpus = Corpus::build(site, lang);
                    out.file(&path, &corpus.json(site.config)?)?;
                    out.wrote_with(&path, format_args!("{} docs", corpus.len()));
                }
                Emitted::Client => {
                    out.file(&path, &Client::standalone(site.config))?;
                    out.wrote(&path);
                }
            }
        }
        Ok(())
    }
}

/// One file this processor writes: a language's index, or the single client
/// every language shares, which picks its index from the page it mounts on.
enum Emitted<'a> {
    Index(&'a str),
    Client,
}

impl<'a> Emitted<'a> {
    /// Every file a build writes, in write order, so what is claimed and what
    /// is written are decided in one place.
    fn plan(config: &'a Config) -> Vec<Self> {
        let indexes = config.langs().into_iter().map(Self::Index);
        indexes
            .chain(config.generate.search.ui.enabled.then_some(Self::Client))
            .collect()
    }

    fn path(&self, config: &Config) -> PathBuf {
        match self {
            Self::Index(lang) => Site::at(config, &[&config.scope(lang, ""), Client::INDEX]),
            Self::Client => Site::at(config, &[Client::FILE]),
        }
    }
}
