//! Client-side search indexes.
//!
//! One [`Corpus`] is built from every page's rendered HTML, then serialized
//! into each configured [`SearchFormat`]. Adding a format is a new match arm
//! plus a [`crate::config::SearchFormat`] variant — the corpus is shared.
//!
//! ## Output schemas
//!
//! - [`SearchFormat::Json`] → `search.json`: a flat array of documents
//!   `[{ "url", "title", "tags", "body" }]`. Feed it to any client library
//!   (Fuse.js, MiniSearch, ..) that indexes at runtime.
//! - [`SearchFormat::Inverted`] → `search.inverted.json`: a prebuilt index
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
        let corpus = Corpus::build(site, &cfg.fields);
        for &format in &cfg.formats {
            let json = match format {
                SearchFormat::Json => corpus.documents_json()?,
                SearchFormat::Inverted => corpus.inverted_json(&cfg.stopwords, cfg.min_length)?,
            };
            out.file(&site.config.dist.join(format.file()), &json)?;
            out.note(format_args!(
                "wrote {} ({} docs)",
                format.file(),
                corpus.len()
            ));
            if cfg.client {
                out.file(
                    &site.config.dist.join(format.client_file()),
                    &format.client(),
                )?;
                out.note(format_args!("wrote {}", format.client_file()));
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
    /// Build a document per page, including only the configured `fields`.
    fn build(site: &Site, fields: &[SearchField]) -> Self {
        let has = |field| fields.contains(&field);
        let documents = site
            .outputs
            .iter()
            .map(|(page, html)| Document {
                url: page.permalink.clone(),
                title: has(SearchField::Title)
                    .then(|| page.frontmatter.title.clone())
                    .flatten()
                    .unwrap_or_default(),
                // every configured taxonomy's terms, not a hardcoded key — a
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
        Self { documents }
    }

    fn len(&self) -> usize {
        self.documents.len()
    }

    /// The flat document list (`search.json`).
    fn documents_json(&self) -> Result<String> {
        json(&self.documents)
    }

    /// A prebuilt inverted index (`search.inverted.json`): term → document ids,
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
/// yields a working Cmd/Ctrl-K palette — no markup or CSS to write. Omitted from
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
    fn client(self) -> String {
        format!("{}\n{PALETTE}{AUTO_MOUNT}", self.engine())
    }

    /// The composable module source (engine + palette, no auto-mount) served to
    /// bundlers through the `baudelaire:search` virtual module.
    pub(crate) fn module(self) -> String {
        format!("{}\n{PALETTE}", self.engine())
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
