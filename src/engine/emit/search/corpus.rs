//! The searchable document set: what of each page is indexed, and the flat
//! `search.json` serialization of it.

use rayon::prelude::*;
use serde::Serialize;

use super::Site;
use crate::config::{SearchConfig, SearchField, SearchFormat};
use crate::engine::text::{Region, Text};
use crate::error::{Artifact, Result};

/// The searchable document set, built once and shared across formats.
pub(super) struct Corpus {
    pub(super) documents: Vec<Document>,
}

impl Corpus {
    /// Build a document per page, including only the configured fields and only
    /// the configured region of each page, ordered by URL.
    ///
    /// The order is load-bearing: the inverted index keys postings by document
    /// *position*, and `site.outputs` is ordered by which pages hit the cache.
    pub(super) fn build(site: &Site, config: &SearchConfig, lang: &str) -> Self {
        let has = |field| config.fields.contains(&field);
        let region = Region::from(&site.config.html.region);
        let mut documents: Vec<Document> = site
            .outputs
            .par_iter()
            .filter(|out| {
                out.page.lang == lang
                    && out.page.listed(site.config)
                    && !out
                        .page
                        .frontmatter
                        .excludes(crate::content::Generated::Search)
            })
            .map(|out| Document {
                url: out.page.permalink.clone(),
                title: has(SearchField::Title)
                    .then(|| out.page.frontmatter.title.clone())
                    .flatten()
                    .unwrap_or_default(),
                tags: if has(SearchField::Tags) {
                    out.page
                        .frontmatter
                        .taxonomies
                        .values()
                        .flatten()
                        .cloned()
                        .collect()
                } else {
                    Vec::new()
                },
                body: if has(SearchField::Body) {
                    Text::extract(out.html, region)
                } else {
                    String::new()
                },
            })
            .collect();
        documents.sort_by(|a, b| a.url.cmp(&b.url));
        Self { documents }
    }

    pub(super) fn len(&self) -> usize {
        self.documents.len()
    }

    /// This corpus serialized in `format`'s shape, as documented at the top of
    /// the parent module.
    pub(super) fn json(&self, format: SearchFormat, cfg: &SearchConfig) -> Result<String> {
        match format {
            SearchFormat::Json => self.documents_json(),
            SearchFormat::Inverted => self.inverted_json(&cfg.stopwords, cfg.min_length),
        }
    }

    /// The flat document list (`search.json`).
    fn documents_json(&self) -> Result<String> {
        Artifact::SearchIndex.json(&self.documents)
    }
}

/// One indexed page. Empty fields are omitted from the JSON.
#[derive(Serialize)]
pub(super) struct Document {
    pub(super) url: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub(super) title: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(super) tags: Vec<String>,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub(super) body: String,
}

impl Document {
    /// Normalized search tokens over every indexed field: split on whitespace,
    /// lowercased, stripped to alphanumerics, empties dropped.
    ///
    /// The client's `tokenize` (in `js/tokenize.js`) must normalize a query
    /// the same way: lowercase *before* stripping, since a codepoint like `İ`
    /// lowercases to a letter plus a combining mark that is not alphanumeric,
    /// and match `char::is_alphanumeric` as `\p{Alphabetic}\p{N}` rather than
    /// the narrower `\p{L}`, which drops marks the index keeps.
    pub(super) fn tokens(&self) -> impl Iterator<Item = String> + '_ {
        std::iter::once(self.title.as_str())
            .chain(std::iter::once(self.body.as_str()))
            .chain(self.tags.iter().map(String::as_str))
            .flat_map(str::split_whitespace)
            .map(Self::normalize)
            .filter(|token| !token.is_empty())
    }

    /// One word reduced to its index key.
    pub(super) fn normalize(word: &str) -> String {
        word.chars()
            .flat_map(char::to_lowercase)
            .filter(|c| c.is_alphanumeric())
            .collect()
    }
}

#[cfg(test)]
impl Document {
    /// One document under a placeholder URL, for the index tests.
    pub(super) fn fake(title: &str, body: &str, tags: &[&str]) -> Self {
        Self {
            url: "/p/".into(),
            title: title.into(),
            tags: tags.iter().map(|s| (*s).to_owned()).collect(),
            body: body.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::emit::Output;

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
        let corpus = |outputs: &[Output]| {
            let site = Site {
                entities: crate::content::Registries::none(),
                config: &config,
                pages: &[],
                outputs,
            };
            let config = SearchConfig {
                fields: vec![SearchField::Title],
                ..SearchConfig::default()
            };
            Corpus::build(&site, &config, "en")
                .documents
                .iter()
                .map(|d| d.url.clone())
                .collect::<Vec<_>>()
        };

        assert_eq!(
            corpus(&[Output::new(&a, ""), Output::new(&b, "")]),
            ["/a/", "/b/"]
        );
        assert_eq!(
            corpus(&[Output::new(&b, ""), Output::new(&a, "")]),
            ["/a/", "/b/"]
        );
    }

    #[test]
    fn documents_json_omits_empty_fields() {
        let corpus = Corpus {
            documents: vec![Document::fake("Title", "", &[])],
        };
        let json = corpus.documents_json().unwrap();
        assert!(json.contains("\"title\":\"Title\""), "{json}");
        assert!(!json.contains("body"), "empty body omitted: {json}");
        assert!(!json.contains("tags"), "empty tags omitted: {json}");
    }
}
