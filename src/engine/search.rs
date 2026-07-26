//! Client-side search indexes.
//!
//! One [`Corpus`] is built from every page's rendered HTML, then serialized
//! into each configured [`SearchFormat`]. Adding a format is a new match arm
//! plus a [`crate::config::SearchFormat`] variant; the corpus is shared.
//!
//! ## Output schemas
//!
//! - [`SearchFormat::Json`] -> `search.json`: a flat array of documents
//!   `[{ "url", "title", "tags", "body" }]`. Feed it to any client library
//!   (Fuse.js, MiniSearch, ..) that indexes at runtime.
//! - [`SearchFormat::Inverted`] -> `search.inverted.json`: a prebuilt index
//!   `{ "documents": [{ "url", "title" }], "postings": { term: [docId..] } }`.
//!   The server does the tokenizing; the client resolves a query by looking up
//!   its terms and intersecting the posting lists.
//!
//! With `search { client true }` each format also emits a matching tiny
//! ES-module client (`search.js` / `search.inverted.js`, see `js/`) exporting
//! `createSearch(url?) -> search(query, { limit })`.

use std::collections::BTreeMap;
use std::collections::HashSet;

use serde::Serialize;

use super::process::{Emit, Processor, Site};
use super::text::Text;
use crate::codegen::{Js, Value};
use crate::config::Permalink;
use crate::config::{Config, SearchField, SearchFormat};
use crate::error::{Artifact, Result, SerializeError};

/// Emits every configured search index format from one shared corpus.
pub(super) struct SearchIndex;

impl Processor for SearchIndex {
    fn enabled(&self, config: &Config) -> bool {
        !config.search.formats.is_empty()
    }

    fn run(&self, site: &Site, out: &mut dyn Emit) -> Result<()> {
        let cfg = &site.config.search;
        // One index per language, alongside that language's feeds. A single
        // global index served English hits to a visitor searching from `/fr/`.
        for lang in site.config.langs() {
            let scope = site.config.scope(lang, "");
            let corpus = Corpus::build(site, &cfg.fields, lang);
            let dir = site.config.dist.join(&scope);
            for &format in &cfg.formats {
                let json = match format {
                    SearchFormat::Json => corpus.documents_json()?,
                    SearchFormat::Inverted => {
                        corpus.inverted_json(&cfg.stopwords, cfg.min_length)?
                    }
                };
                out.file(&dir.join(format.file()), &json)?;
                out.note(format_args!(
                    "wrote {}/{} ({} docs)",
                    scope,
                    format.file(),
                    corpus.len()
                ));
                if cfg.client {
                    out.file(
                        &dir.join(format.client_file()),
                        &format.client(site.config.base_path(), &format.index(site.config, lang)),
                    )?;
                    out.note(format_args!("wrote {}/{}", scope, format.client_file()));
                }
            }
        }
        Ok(())
    }
}

/// The searchable document set, built once and shared across formats.
struct Corpus {
    documents: Vec<Document>,
}

impl Corpus {
    /// Build a document per page, including only the configured `fields`,
    /// ordered by URL.
    ///
    /// The order is load-bearing: the inverted index keys postings by document
    /// *position*, and `site.outputs` is ordered by which pages hit the cache.
    fn build(site: &Site, fields: &[SearchField], lang: &str) -> Self {
        let has = |field| fields.contains(&field);
        let mut documents: Vec<Document> = site
            .outputs
            .iter()
            .filter(|(page, _)| page.lang == lang)
            .map(|(page, html)| Document {
                url: page.permalink.clone(),
                title: has(SearchField::Title)
                    .then(|| page.frontmatter.title.clone())
                    .flatten()
                    .unwrap_or_default(),
                // every configured taxonomy's terms, not a hardcoded key: a
                // site classifying by `topics` indexes just as well as `tags`.
                tags: if has(SearchField::Tags) {
                    page.frontmatter
                        .taxonomies
                        .values()
                        .flatten()
                        .cloned()
                        .collect()
                } else {
                    Vec::new()
                },
                body: if has(SearchField::Body) {
                    Text::extract(html)
                } else {
                    String::new()
                },
            })
            .collect();
        documents.sort_by(|a, b| a.url.cmp(&b.url));
        Self { documents }
    }

    fn len(&self) -> usize {
        self.documents.len()
    }

    /// The flat document list (`search.json`).
    fn documents_json(&self) -> Result<String> {
        json(&self.documents)
    }

    /// A prebuilt inverted index (`search.inverted.json`): term -> document ids,
    /// with tokens shorter than `min_length` or listed in `stopwords` dropped.
    fn inverted_json(&self, stopwords: &[String], min_length: usize) -> Result<String> {
        let stop: HashSet<&str> = stopwords.iter().map(String::as_str).collect();
        let mut postings: BTreeMap<String, Vec<usize>> = BTreeMap::new();
        for (id, doc) in self.documents.iter().enumerate() {
            for token in doc.tokens() {
                if token.len() < min_length || stop.contains(token.as_str()) {
                    continue;
                }
                let ids = postings.entry(token).or_default();
                // one doc's tokens are visited contiguously, so checking the last
                // id keeps each posting list duplicate-free.
                if ids.last() != Some(&id) {
                    ids.push(id);
                }
            }
        }
        let index = Inverted {
            documents: self.documents.iter().map(Meta::from).collect(),
            postings,
        };
        json(&index)
    }
}

/// One indexed page. Empty fields are omitted from the JSON.
#[derive(Serialize)]
struct Document {
    url: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    title: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tags: Vec<String>,
    #[serde(skip_serializing_if = "String::is_empty")]
    body: String,
}

impl Document {
    /// Normalized search tokens over every indexed field: lowercased, split on
    /// whitespace, stripped to alphanumerics, empties dropped.
    fn tokens(&self) -> impl Iterator<Item = String> + '_ {
        std::iter::once(self.title.as_str())
            .chain(std::iter::once(self.body.as_str()))
            .chain(self.tags.iter().map(String::as_str))
            .flat_map(str::split_whitespace)
            .map(|word| {
                word.chars()
                    .filter(|c| c.is_alphanumeric())
                    .flat_map(char::to_lowercase)
                    .collect::<String>()
            })
            .filter(|token| !token.is_empty())
    }
}

/// The display metadata carried in an inverted index (the body lives only in
/// the postings, not repeated per document).
#[derive(Serialize)]
struct Meta<'a> {
    url: &'a str,
    title: &'a str,
}

impl<'a> From<&'a Document> for Meta<'a> {
    fn from(doc: &'a Document) -> Self {
        Self {
            url: &doc.url,
            title: &doc.title,
        }
    }
}

/// A prebuilt inverted index.
#[derive(Serialize)]
struct Inverted<'a> {
    documents: Vec<Meta<'a>>,
    postings: BTreeMap<String, Vec<usize>>,
}

/// Serialize a search artifact to JSON, tagging any failure as the search index.
fn json<T: Serialize>(value: &T) -> Result<String> {
    serde_json::to_string(value).map_err(|e| SerializeError::new(Artifact::SearchIndex, e).into())
}

/// The self-mounting command-palette UI, concatenated onto whichever engine a
/// format needs. Shared verbatim by every format and by the virtual module.
const PALETTE: &str = include_str!("js/palette.js");

/// Appended to the standalone client so dropping one `<script type=module>`
/// yields a working Cmd/Ctrl-K palette: no markup or CSS to write. Omitted from
/// the virtual-module source, where the importer wires the trigger itself.
const AUTO_MOUNT: &str = "\nif (typeof document !== \"undefined\") mountSearch();\n";

/// Generated JavaScript for a search format. The engine (defining `createSearch`)
/// and the shared [`PALETTE`] UI are real `.js` sources under `js/`, embedded and
/// concatenated so the two composable pieces stay in one module scope.
impl SearchFormat {
    /// The per-format engine source, defining `createSearch`.
    fn engine(self) -> &'static str {
        match self {
            Self::Json => include_str!("js/engine.flat.js"),
            Self::Inverted => include_str!("js/engine.inverted.js"),
        }
    }

    /// The standalone generated client: engine + palette UI + auto-mount.
    fn client(self, base: &str, index: &str) -> String {
        format!(
            "{}{}\n{PALETTE}{AUTO_MOUNT}",
            Self::prelude(base, index),
            self.engine()
        )
    }

    /// The composable module source (engine + palette, no auto-mount) served to
    /// bundlers through the `baudelaire:search` virtual module.
    #[cfg(feature = "js")]
    pub(crate) fn module(self, base: &str, index: &str) -> String {
        format!("{}{}\n{PALETTE}", Self::prelude(base, index), self.engine())
    }

    /// The served URL of this format's index for `lang`: what the generated
    /// client fetches, and the single spelling shared by the emitted client and
    /// the `baudelaire:search` virtual module.
    pub(crate) fn index(self, config: &Config, lang: &str) -> String {
        let dir = config.prefixed(&Permalink::join(&[&config.scope(lang, "")]));
        format!("{dir}{}", self.file())
    }

    /// The two constants the generated client closes over: `BASE`, prepended to
    /// each hit's href so a subdirectory-hosted site resolves it, and `INDEX`,
    /// the URL of the index this client fetches.
    ///
    /// Separate because they scope differently: hits carry already-localized
    /// permalinks, so folding the language into `BASE` would double it.
    fn prelude(base: &str, index: &str) -> String {
        format!(
            "const BASE = {};\nconst INDEX = {};\n",
            Js(&Value::str(base)),
            Js(&Value::str(index))
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc(title: &str, body: &str, tags: &[&str]) -> Document {
        Document {
            url: "/p/".into(),
            title: title.into(),
            tags: tags.iter().map(|s| (*s).to_owned()).collect(),
            body: body.into(),
        }
    }

    /// Document ids index the inverted postings, so the corpus must not
    /// inherit `site.outputs`' cache-split order: a cold and an incremental
    /// build of identical content have to produce byte-identical indexes.
    #[test]
    fn corpus_is_ordered_by_url_not_by_cache_split() {
        use crate::config::Config;
        use crate::content::{Data, Frontmatter, Page, PageId, Siblings};
        use std::path::PathBuf;

        let page = |slug: &str| Page {
            id: PageId::new("posts", slug),
            source: PathBuf::from(format!("content/{slug}.typ")),
            frontmatter: Frontmatter::default(),
            body: String::new(),
            data: Data::Empty,
            collection: "posts".into(),
            permalink: format!("/{slug}/"),
            output: PathBuf::new(),
            template: None,
            lang: "en".into(),
            siblings: Siblings::default(),
            translations: Vec::new(),
        };
        let (a, b) = (page("a"), page("b"));
        let config = Config::default();
        let corpus = |outputs: &[(&Page, &str)]| {
            let site = Site {
                config: &config,
                pages: &[],
                outputs,
            };
            Corpus::build(&site, &[SearchField::Title], "en")
                .documents
                .iter()
                .map(|d| d.url.clone())
                .collect::<Vec<_>>()
        };

        assert_eq!(corpus(&[(&a, ""), (&b, "")]), ["/a/", "/b/"]);
        assert_eq!(corpus(&[(&b, ""), (&a, "")]), ["/a/", "/b/"]);
    }

    #[test]
    fn documents_json_omits_empty_fields() {
        let corpus = Corpus {
            documents: vec![doc("Title", "", &[])],
        };
        let json = corpus.documents_json().unwrap();
        assert!(json.contains("\"title\":\"Title\""), "{json}");
        assert!(!json.contains("body"), "empty body omitted: {json}");
        assert!(!json.contains("tags"), "empty tags omitted: {json}");
    }

    #[test]
    fn inverted_index_tokenizes_and_maps_terms_to_docs() {
        let corpus = Corpus {
            documents: vec![
                doc("Rust", "rust is fast", &[]),
                doc("Go", "go is fast", &[]),
            ],
        };
        let json = corpus.inverted_json(&["is".into()], 2).unwrap();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        let postings = &value["postings"];
        // "fast" appears in both docs; "rust" only in the first.
        assert_eq!(postings["fast"], serde_json::json!([0, 1]));
        assert_eq!(postings["rust"], serde_json::json!([0]));
        // Stopword "is" and sub-min-length tokens are excluded.
        assert!(postings.get("is").is_none(), "stopword dropped: {json}");
    }
}
