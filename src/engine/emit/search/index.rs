//! The prebuilt inverted index: term -> document ids, beside the display
//! metadata a hit needs.

use std::collections::BTreeMap;
use std::collections::HashSet;

use rayon::prelude::*;
use serde::Serialize;

use super::corpus::{Corpus, Document};
use crate::error::{Artifact, Result};

impl Corpus {
    /// A prebuilt inverted index (`search.inverted.json`): term -> document
    /// ids, with tokens shorter than `min_length` or in `stopwords` dropped.
    pub(super) fn inverted_json(&self, stopwords: &[String], min_length: usize) -> Result<String> {
        let stop: HashSet<&str> = stopwords.iter().map(String::as_str).collect();
        let postings = self
            .documents
            .par_iter()
            .enumerate()
            .fold(Postings::default, |mut postings, (id, doc)| {
                postings.add(id, doc, &stop, min_length);
                postings
            })
            .reduce(Postings::default, Postings::merge);
        let index = Inverted {
            documents: self.documents.iter().map(Meta::from).collect(),
            postings,
        };
        Artifact::SearchIndex.json(&index)
    }
}

/// Which documents carry each term, their ids ascending: the half of an
/// inverted index a query looks a term up in.
///
/// A posting list is sorted and holds each id once, which the client relies on
/// however the parallel build chunked it.
#[derive(Default, Serialize)]
#[serde(transparent)]
struct Postings(BTreeMap<String, Vec<usize>>);

impl Postings {
    /// Index one document under every term it carries, skipping tokens the site
    /// excludes.
    fn add(&mut self, id: usize, doc: &Document, stop: &HashSet<&str>, min_length: usize) {
        for token in doc.tokens() {
            if token.len() < min_length || stop.contains(token.as_str()) {
                continue;
            }
            let ids = self.0.entry(token).or_default();
            if ids.last() != Some(&id) {
                ids.push(id);
            }
        }
    }

    /// Fold `later` into these, sorting only where the two ranges are not
    /// already in order.
    fn merge(mut self, later: Self) -> Self {
        for (term, ids) in later.0 {
            let list = self.0.entry(term).or_default();
            let ordered = match (list.last(), ids.first()) {
                (Some(seen), Some(next)) => seen < next,
                _ => true,
            };
            list.extend(ids);
            if !ordered {
                list.sort_unstable();
                list.dedup();
            }
        }
        self
    }
}

/// The display metadata carried in an inverted index; the body lives only in
/// the postings.
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
    postings: Postings,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inverted_index_tokenizes_and_maps_terms_to_docs() {
        let corpus = Corpus {
            documents: vec![
                Document::fake("Rust", "rust is fast", &[]),
                Document::fake("Go", "go is fast", &[]),
            ],
        };
        let json = corpus.inverted_json(&["is".into()], 2).unwrap();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        let postings = &value["postings"];
        assert_eq!(postings["fast"], serde_json::json!([0, 1]));
        assert_eq!(postings["rust"], serde_json::json!([0]));
        assert!(postings.get("is").is_none(), "stopword dropped: {json}");
    }

    #[test]
    fn merged_posting_lists_stay_sorted_and_unique() {
        let postings = |pairs: &[(&str, &[usize])]| {
            Postings(
                pairs
                    .iter()
                    .map(|(term, ids)| ((*term).to_owned(), ids.to_vec()))
                    .collect(),
            )
        };
        let merged =
            |a: &[(&str, &[usize])], b: &[(&str, &[usize])]| postings(a).merge(postings(b)).0;

        assert_eq!(merged(&[("t", &[0, 1])], &[("t", &[2])])["t"], [0, 1, 2]);
        assert_eq!(merged(&[("t", &[2])], &[("t", &[0, 1])])["t"], [0, 1, 2]);
        assert_eq!(merged(&[("t", &[0, 1])], &[("t", &[1, 2])])["t"], [0, 1, 2]);
        let both = merged(&[("a", &[0])], &[("b", &[1])]);
        assert_eq!(
            (both["a"].as_slice(), both["b"].as_slice()),
            (&[0][..], &[1][..])
        );
    }
}
