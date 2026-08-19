//! The serialized index: the header every client reads, and the postings the
//! `terms` shape prebuilds.

use std::collections::BTreeMap;
use std::collections::HashMap;

use rayon::prelude::*;
use serde::Serialize;

use super::corpus::{Corpus, Document};
use super::tokens::Tokens;
use crate::config::{Config, SearchConfig, SearchFields};
use crate::error::{Artifact, Result};

/// The index format the client is written against; a client reading an older
/// one says so rather than finding nothing.
const VERSION: u32 = 1;

impl Corpus {
    /// This corpus as `search.json`, in the shape `generate { search { index } }`
    /// names.
    pub(super) fn json(&self, config: &Config) -> Result<String> {
        let search = &config.generate.search;
        let index = Index {
            version: VERSION,
            base: config.base_path(),
            snippet: search.snippet,
            documents: self
                .documents
                .iter()
                .map(|doc| doc.stored(search))
                .collect(),
            shape: if search.indexed_here() {
                Shape::Terms(self.postings(search))
            } else {
                Shape::Documents {
                    weights: Weights::from(&search.fields),
                    minimum: search.min_length,
                    stopwords: &search.stopwords,
                }
            },
        };
        Artifact::SearchIndex.json(&index)
    }

    /// Every term the corpus carries and what it scored in each document that
    /// carries it.
    fn postings(&self, config: &SearchConfig) -> Postings {
        self.documents
            .par_iter()
            .enumerate()
            .fold(Postings::default, |mut postings, (id, doc)| {
                postings.add(id, doc, config);
                postings
            })
            .reduce(Postings::default, Postings::merge)
    }
}

/// One document as the index carries it: enough to render a hit, and under the
/// `documents` shape enough to find one.
#[derive(Serialize)]
struct Stored<'a> {
    url: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    title: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tags: Option<&'a [String]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    text: Option<&'a str>,
}

impl Document {
    /// This document as the index carries it.
    fn stored<'a>(&'a self, config: &SearchConfig) -> Stored<'a> {
        Stored {
            url: &self.url,
            title: (!self.title.is_empty()).then_some(self.title.as_str()),
            tags: (!self.tags.is_empty()).then_some(self.tags.as_slice()),
            text: self.prose(config),
        }
    }
}

/// A written index: the header both shapes carry, and the half that differs.
#[derive(Serialize)]
struct Index<'a> {
    version: u32,
    /// The site's base path, which a hit's href is written under. It travels
    /// with the index rather than in the client, so one client serves every
    /// language and every mount point.
    base: &'a str,
    snippet: usize,
    documents: Vec<Stored<'a>>,
    #[serde(flatten)]
    shape: Shape<'a>,
}

/// What a client has to do to answer a query: look a term up, or index the
/// documents first under the rules the build would have used.
#[derive(Serialize)]
#[serde(tag = "index", rename_all = "lowercase")]
enum Shape<'a> {
    Terms(Postings),
    Documents {
        weights: Weights,
        minimum: usize,
        stopwords: &'a [String],
    },
}

/// What a match in each part of a page is worth, as the client reads them.
#[derive(Serialize)]
struct Weights {
    title: usize,
    tags: usize,
    body: usize,
}

impl From<&SearchFields> for Weights {
    fn from(fields: &SearchFields) -> Self {
        Self {
            title: fields.title,
            tags: fields.tags,
            body: fields.body,
        }
    }
}

/// Which documents carry each term and what it scored there, their ids
/// ascending: the half of an index a query looks a term up in.
///
/// Serialized as two parallel arrays, the terms sorted, so a client resolves a
/// prefix by binary search instead of scanning every key.
#[derive(Default)]
struct Postings(BTreeMap<String, Vec<[usize; 2]>>);

impl Serialize for Postings {
    fn serialize<S: serde::Serializer>(
        &self,
        serializer: S,
    ) -> std::result::Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut out = serializer.serialize_struct("Postings", 2)?;
        out.serialize_field("terms", &self.0.keys().collect::<Vec<_>>())?;
        out.serialize_field("postings", &self.0.values().collect::<Vec<_>>())?;
        out.end()
    }
}

impl Postings {
    /// Index one document under every term it carries, scoring each by the
    /// weight of the fields it appeared in and how often.
    fn add(&mut self, id: usize, doc: &Document, config: &SearchConfig) {
        let mut scores: HashMap<String, usize> = HashMap::new();
        for (text, weight) in doc.fields(&config.fields) {
            for token in Tokens::of(text) {
                if Tokens::kept(&token, config.min_length, &config.stopwords) {
                    *scores.entry(token).or_default() += weight;
                }
            }
        }
        for (term, score) in scores {
            self.0.entry(term).or_default().push([id, score]);
        }
    }

    /// Fold `later` into these, sorting only where the two ranges are not
    /// already in order.
    fn merge(mut self, later: Self) -> Self {
        for (term, entries) in later.0 {
            let list = self.0.entry(term).or_default();
            let ordered = match (list.last(), entries.first()) {
                (Some(seen), Some(next)) => seen[0] < next[0],
                _ => true,
            };
            list.extend(entries);
            if !ordered {
                list.sort_unstable();
                list.dedup();
            }
        }
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::SearchIndex;

    fn corpus() -> Corpus {
        Corpus {
            documents: vec![
                Document::fake("Rust", "rust is fast", &[]),
                Document::fake("Go", "go is fast", &[]),
            ],
        }
    }

    fn json(config: SearchConfig) -> serde_json::Value {
        let mut site = Config::default();
        site.generate.search = config;
        serde_json::from_str(&corpus().json(&site).unwrap()).unwrap()
    }

    #[test]
    fn terms_are_scored_by_the_weight_of_the_fields_they_appear_in() {
        let value = json(SearchConfig {
            enabled: true,
            stopwords: vec!["is".into()],
            ..SearchConfig::default()
        });
        let terms: Vec<&str> = value["terms"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t.as_str().unwrap())
            .collect();
        assert!(!terms.contains(&"is"), "stopword dropped: {terms:?}");
        let at = |term: &str| terms.iter().position(|t| *t == term).expect(term);
        // "rust" is the title of document 0 and appears in its prose: 5 + 1.
        assert_eq!(value["postings"][at("rust")], serde_json::json!([[0, 6]]));
        assert_eq!(
            value["postings"][at("fast")],
            serde_json::json!([[0, 1], [1, 1]])
        );
    }

    #[test]
    fn the_documents_shape_ships_the_rules_instead_of_the_postings() {
        let value = json(SearchConfig {
            enabled: true,
            index: SearchIndex::Documents,
            ..SearchConfig::default()
        });
        assert_eq!(value["index"], "documents");
        assert_eq!(value["weights"]["title"], 5);
        assert_eq!(value["minimum"], 2);
        assert!(value.get("postings").is_none(), "{value}");
        assert_eq!(value["documents"][0]["text"], "rust is fast");
    }

    #[test]
    fn merged_posting_lists_stay_sorted_and_unique() {
        let postings = |pairs: &[(&str, &[[usize; 2]])]| {
            Postings(
                pairs
                    .iter()
                    .map(|(term, ids)| ((*term).to_owned(), ids.to_vec()))
                    .collect(),
            )
        };
        let merged = |a: &[(&str, &[[usize; 2]])], b: &[(&str, &[[usize; 2]])]| {
            postings(a).merge(postings(b)).0
        };

        assert_eq!(
            merged(&[("t", &[[0, 1], [1, 1]])], &[("t", &[[2, 1]])])["t"],
            [[0, 1], [1, 1], [2, 1]]
        );
        assert_eq!(
            merged(&[("t", &[[2, 1]])], &[("t", &[[0, 1], [1, 1]])])["t"],
            [[0, 1], [1, 1], [2, 1]]
        );
        let both = merged(&[("a", &[[0, 1]])], &[("b", &[[1, 1]])]);
        assert_eq!(
            (both["a"].as_slice(), both["b"].as_slice()),
            (&[[0, 1]][..], &[[1, 1]][..])
        );
    }
}
