//! Client-side search indexes: one [`Corpus`] built from every page's rendered
//! HTML, serialized into each configured [`SearchFormat`].
//!
//! - [`SearchFormat::Json`] -> `search.json`: a flat array of documents
//!   `[{ "url", "title", "tags", "body" }]`.
//! - [`SearchFormat::Inverted`] -> `search.inverted.json`: a prebuilt index
//!   `{ "documents": [{ "url", "title" }], "postings": { term: [docId..] } }`.

use std::collections::BTreeMap;
use std::collections::HashSet;

use rayon::prelude::*;
use serde::Serialize;

use super::script::Script;
use super::{Emit, Processor, Site};
use crate::config::Permalink;
use crate::config::{Config, SearchConfig, SearchField, SearchFormat};
use crate::engine::text::{Region, Text};
use crate::error::{Artifact, Result};

/// Emits every configured search index format from one shared corpus.
pub(super) struct SearchIndex;

impl Processor for SearchIndex {
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
                out.file(&path, &format.json(&corpus, cfg)?)?;
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

/// The searchable document set, built once and shared across formats.
struct Corpus {
    documents: Vec<Document>,
}

impl Corpus {
    /// Build a document per page, including only the configured fields and only
    /// the configured region of each page, ordered by URL.
    ///
    /// The order is load-bearing: the inverted index keys postings by document
    /// *position*, and `site.outputs` is ordered by which pages hit the cache.
    fn build(site: &Site, config: &SearchConfig, lang: &str) -> Self {
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

    fn len(&self) -> usize {
        self.documents.len()
    }

    /// The flat document list (`search.json`).
    fn documents_json(&self) -> Result<String> {
        Artifact::SearchIndex.json(&self.documents)
    }

    /// A prebuilt inverted index (`search.inverted.json`): term -> document
    /// ids, with tokens shorter than `min_length` or in `stopwords` dropped.
    fn inverted_json(&self, stopwords: &[String], min_length: usize) -> Result<String> {
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
    /// Normalized search tokens over every indexed field: split on whitespace,
    /// lowercased, stripped to alphanumerics, empties dropped.
    ///
    /// The client's `tokenize` (in `js/tokenize.js`) must normalize a query
    /// the same way: lowercase *before* stripping, since a codepoint like `İ`
    /// lowercases to a letter plus a combining mark that is not alphanumeric,
    /// and match `char::is_alphanumeric` as `\p{Alphabetic}\p{N}` rather than
    /// the narrower `\p{L}`, which drops marks the index keeps.
    fn tokens(&self) -> impl Iterator<Item = String> + '_ {
        std::iter::once(self.title.as_str())
            .chain(std::iter::once(self.body.as_str()))
            .chain(self.tags.iter().map(String::as_str))
            .flat_map(str::split_whitespace)
            .map(Self::normalize)
            .filter(|token| !token.is_empty())
    }

    /// One word reduced to its index key.
    fn normalize(word: &str) -> String {
        word.chars()
            .flat_map(char::to_lowercase)
            .filter(|c| c.is_alphanumeric())
            .collect()
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

/// The query tokenizer, shared by both engines and by the palette.
const TOKENIZE: &str = include_str!("js/tokenize.js");

/// The self-mounting command-palette UI, concatenated onto whichever engine a
/// format needs.
const PALETTE: &str = include_str!("js/palette.js");

/// The generated client's entry point.
const MOUNT: &str = "mountSearch";

/// Generated JavaScript for a search format: the engine, the shared
/// [`TOKENIZE`] rule and the [`PALETTE`] UI, concatenated into one module
/// scope.
impl SearchFormat {
    /// The per-format engine source, defining `createSearch`.
    fn engine(self) -> &'static str {
        match self {
            Self::Json => include_str!("js/engine.flat.js"),
            Self::Inverted => include_str!("js/engine.inverted.js"),
        }
    }

    /// This format's serialized index over `corpus`, in the shape documented at
    /// the top of this module.
    fn json(self, corpus: &Corpus, cfg: &SearchConfig) -> Result<String> {
        match self {
            Self::Json => corpus.documents_json(),
            Self::Inverted => corpus.inverted_json(&cfg.stopwords, cfg.min_length),
        }
    }

    /// The standalone generated client: tokenizer, engine and palette UI, with
    /// an auto-mount.
    fn client(self, base: &str, index: &str) -> String {
        self.script(base, index).mount(MOUNT)
    }

    /// The composable module source served to bundlers through the
    /// `baudelaire:search` virtual module, with no auto-mount.
    #[cfg(feature = "js")]
    pub(crate) fn module(self, base: &str, index: &str) -> String {
        self.script(base, index).finish()
    }

    /// The sources every build of this format's client is assembled from, and
    /// the two constants they close over: `BASE`, prepended to each hit's href,
    /// and `INDEX`, the URL of the index this client fetches.
    ///
    /// The two are separate because a hit carries an already-localized
    /// permalink, so folding the language into `BASE` would double it.
    fn script(self, base: &str, index: &str) -> Script<'static> {
        Script::new(&[("BASE", base), ("INDEX", index)])
            .part(TOKENIZE)
            .part(self.engine())
            .part(PALETTE)
    }

    /// The served URL of this format's index for `lang`, which the generated
    /// client fetches.
    pub(crate) fn index(self, config: &Config, lang: &str) -> String {
        let dir = config.prefixed(&Permalink::join(&[&config.scope(lang, "")]));
        format!("{dir}{}", self.file())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::emit::Output;

    /// Every case below is one where the two tokenizers *disagreed*: a `\p{L}`
    /// client, or a strip-then-lowercase index, fails this test.
    #[test]
    fn tokens_agree_with_the_client_tokenizer() {
        assert_eq!(Document::normalize("İstanbul"), "istanbul");
        assert_eq!(Document::normalize("हिन्दी"), "हिनदी");
        assert_eq!(Document::normalize("مُحَمَّد"), "مُحَمَّد");
        assert_eq!(Document::normalize("שָׁלוֹם"), "שָׁלוֹם");
        assert_eq!(Document::normalize("ÅNGSTRÖM"), "ångström");
        assert_eq!(Document::normalize("ǅungla"), "ǆungla");
        assert_eq!(Document::normalize("foo-bar!"), "foobar");
        assert_eq!(Document::normalize("x²"), "x²");
        assert_eq!(Document::normalize("--"), "");

        assert!(
            TOKENIZE.contains(r"[^\p{Alphabetic}\p{N}]"),
            "the client must retain exactly `char::is_alphanumeric`; `\\p{{L}}` \
             drops the combining marks the index keeps"
        );
        let (lower, strip) = (
            TOKENIZE.find("toLowerCase").expect("client lowercases"),
            TOKENIZE.find("replace").expect("client strips"),
        );
        assert!(
            lower < strip,
            "the client must lowercase before stripping, as `Document::normalize` does"
        );
    }

    fn doc(title: &str, body: &str, tags: &[&str]) -> Document {
        Document {
            url: "/p/".into(),
            title: title.into(),
            tags: tags.iter().map(|s| (*s).to_owned()).collect(),
            body: body.into(),
        }
    }

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
