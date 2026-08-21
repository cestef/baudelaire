//! The searchable document set: what of each page the index carries, and what
//! of it a query scores against.

use rayon::prelude::*;

use super::Site;
use crate::config::{SearchConfig, SearchFields};
use crate::engine::text::{Region, Text};

/// One language's documents, in URL order.
pub(super) struct Corpus {
    pub(super) documents: Vec<Document>,
}

impl Corpus {
    /// Build a document per listed page of `lang`, reading only the configured
    /// region of each, ordered by URL.
    ///
    /// The order is load-bearing: postings key documents by *position*, and
    /// `site.outputs` is ordered by which pages hit the cache.
    pub(super) fn build(site: &Site, lang: &str) -> Self {
        let config = &site.config.generate.search;
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
                title: out.page.frontmatter.title.clone().unwrap_or_default(),
                tags: if config.fields.tags > 0 {
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
                body: if config.carries_prose() {
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
}

/// One indexed page, holding its prose whole; how much of that prose ships is
/// the index's decision.
pub(super) struct Document {
    pub(super) url: String,
    pub(super) title: String,
    pub(super) tags: Vec<String>,
    pub(super) body: String,
}

impl Document {
    /// Each indexed part of the page with what a match in it is worth, the
    /// parts a site zeroed left out.
    pub(super) fn fields<'a>(
        &'a self,
        weights: &'a SearchFields,
    ) -> impl Iterator<Item = (&'a str, usize)> + 'a {
        [
            (self.title.as_str(), weights.title),
            (self.body.as_str(), weights.body),
        ]
        .into_iter()
        .chain(self.tags.iter().map(|tag| (tag.as_str(), weights.tags)))
        .filter(|(text, weight)| *weight > 0 && !text.is_empty())
    }

    /// The prose this document ships, which is all of it where the client does
    /// the indexing and at most `limit` characters where it does not.
    pub(super) fn prose(&self, config: &SearchConfig) -> Option<&str> {
        let limit = if config.indexed_here() {
            self.body
                .char_indices()
                .nth(config.snippet)
                .map_or(self.body.len(), |(at, _)| at)
        } else {
            self.body.len()
        };
        let text = self.body[..limit].trim_end();
        (!text.is_empty()).then_some(text)
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
        use crate::content::{Data, Frontmatter, Page, PageId};
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
        };
        let (a, b) = (page("a"), page("b"));
        let config = Config::default();
        let corpus = |outputs: &[Output]| {
            let site = Site {
                entities: crate::content::Registries::none(),
                relations: crate::content::Relations::none(),
                history: crate::git::History::none(),
                config: &config,
                pages: &[],
                outputs,
            };
            Corpus::build(&site, "en")
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
    fn stored_prose_is_whole_where_the_client_indexes_it() {
        let doc = Document::fake("T", "alpha beta gamma delta", &[]);
        let clipped = SearchConfig {
            snippet: 10,
            ..SearchConfig::default()
        };
        assert_eq!(doc.prose(&clipped), Some("alpha beta"));
        let whole = SearchConfig {
            index: crate::config::SearchIndex::Documents,
            ..clipped
        };
        assert_eq!(doc.prose(&whole), Some("alpha beta gamma delta"));
    }

    #[test]
    fn a_zero_snippet_ships_no_prose() {
        let doc = Document::fake("T", "alpha", &[]);
        let config = SearchConfig {
            snippet: 0,
            ..SearchConfig::default()
        };
        assert_eq!(doc.prose(&config), None);
    }
}
