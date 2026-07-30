//! Client-side search indexes.
//!
//! One [`Corpus`] is built from every page's rendered HTML, then serialized
//! into each configured [`SearchFormat`]. Adding a format is a
//! [`crate::config::SearchFormat`] variant plus an arm in each method of the
//! `impl SearchFormat` below, which is where everything a format decides lives;
//! the corpus is shared.
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
//! With `generate { search { client #true } }` each format also emits a matching tiny
//! ES-module client (`search.js` / `search.inverted.js`, see `js/`) exporting
//! `createSearch(url?) -> search(query, { limit })`.

use std::collections::BTreeMap;
use std::collections::HashSet;

use serde::Serialize;

use super::script::Script;
use super::{Emit, Processor, Site};
use crate::config::Permalink;
use crate::config::{Config, SearchConfig, SearchField, SearchFormat};
use crate::engine::text::Text;
use crate::error::{Artifact, Result};

/// Emits every configured search index format from one shared corpus.
pub(super) struct SearchIndex;

impl Processor for SearchIndex {
    fn enabled(&self, config: &Config) -> bool {
        !config.generate.search.formats.is_empty()
    }

    fn run(&self, site: &Site, out: &mut dyn Emit) -> Result<()> {
        let cfg = &site.config.generate.search;
        // One index per language, alongside that language's feeds. A single
        // global index served English hits to a visitor searching from `/fr/`.
        for lang in site.config.langs() {
            let scope = site.config.scope(lang, "");
            let corpus = Corpus::build(site, &cfg.fields, lang);
            for &format in &cfg.formats {
                out.file(
                    &site.dist(&[&scope, format.file()]),
                    &format.json(&corpus, cfg)?,
                )?;
                out.note(format_args!(
                    "wrote {}/{} ({} docs)",
                    scope,
                    format.file(),
                    corpus.len()
                ));
                if cfg.ui {
                    out.file(
                        &site.dist(&[&scope, format.client_file()]),
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
            .filter(|out| out.page.lang == lang)
            .map(|out| Document {
                url: out.page.permalink.clone(),
                title: has(SearchField::Title)
                    .then(|| out.page.frontmatter.title.clone())
                    .flatten()
                    .unwrap_or_default(),
                // every configured taxonomy's terms, not a hardcoded key: a
                // site classifying by `topics` indexes just as well as `tags`.
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
                    Text::extract(out.html)
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
        Artifact::SearchIndex.json(&index)
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
    /// The client's `tokenize` (in `js/tokenize.js`) has to normalize a query
    /// the same way, or a term is looked up in a form the postings were never
    /// keyed by. Two things make that hold, and both are load-bearing:
    ///
    /// - **Lowercase before stripping.** `İ` lowercases to `i` + U+0307, and
    ///   U+0307 is not alphanumeric. Stripping first never sees the mark it is
    ///   about to introduce, so the index was keyed `i̇stanbul` while every query
    ///   asked for `istanbul`, and the page was unreachable.
    /// - **`char::is_alphanumeric` is Unicode `Alphabetic | N`,** which the
    ///   client spells `\p{Alphabetic}\p{N}`. The narrower-looking `\p{L}` is
    ///   *not* the same set: it drops the Indic, Arabic and Hebrew vowel marks
    ///   this keeps, so a Devanagari word indexed whole was queried stripped.
    ///
    /// [`tokens_agree_with_the_client_tokenizer`] pins both against the cases
    /// that broke.
    ///
    /// [`tokens_agree_with_the_client_tokenizer`]: tests::tokens_agree_with_the_client_tokenizer
    fn tokens(&self) -> impl Iterator<Item = String> + '_ {
        std::iter::once(self.title.as_str())
            .chain(std::iter::once(self.body.as_str()))
            .chain(self.tags.iter().map(String::as_str))
            .flat_map(str::split_whitespace)
            .map(Self::normalize)
            .filter(|token| !token.is_empty())
    }

    /// One word reduced to its index key. Split out from [`tokens`](Self::tokens)
    /// so the agreement test can drive it directly, rather than round-tripping a
    /// whole document to see one word.
    fn normalize(word: &str) -> String {
        word.chars()
            .flat_map(char::to_lowercase)
            .filter(|c| c.is_alphanumeric())
            .collect()
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

/// The query tokenizer, one definition for both engines and for the palette
/// that highlights what a query matched.
const TOKENIZE: &str = include_str!("js/tokenize.js");

/// The self-mounting command-palette UI, concatenated onto whichever engine a
/// format needs. Shared verbatim by every format and by the virtual module.
const PALETTE: &str = include_str!("js/palette.js");

/// The generated client's entry point, called by the emitted standalone file
/// and exported to bundlers by the virtual module.
const MOUNT: &str = "mountSearch";

/// Generated JavaScript for a search format. The engine (defining `createSearch`),
/// the shared [`TOKENIZE`] rule and the [`PALETTE`] UI are real `.js` sources
/// under `js/`, embedded and concatenated so the composable pieces stay in one
/// module scope.
impl SearchFormat {
    /// The per-format engine source, defining `createSearch`.
    fn engine(self) -> &'static str {
        match self {
            Self::Json => include_str!("js/engine.flat.js"),
            Self::Inverted => include_str!("js/engine.inverted.js"),
        }
    }

    /// This format's serialized index over `corpus`: the shape documented at
    /// the top of this module. Here rather than at the call site so everything
    /// a format decides (its file names, its engine, its index shape) is stated
    /// in this one impl.
    fn json(self, corpus: &Corpus, cfg: &SearchConfig) -> Result<String> {
        match self {
            Self::Json => corpus.documents_json(),
            Self::Inverted => corpus.inverted_json(&cfg.stopwords, cfg.min_length),
        }
    }

    /// The standalone generated client: tokenizer + engine + palette UI, with an
    /// auto-mount so dropping one `<script type=module>` yields a working
    /// Cmd/Ctrl-K palette and no markup or CSS to write.
    fn client(self, base: &str, index: &str) -> String {
        self.script(base, index).mount(MOUNT)
    }

    /// The composable module source (no auto-mount, the importer wires the
    /// trigger itself) served to bundlers through the `baudelaire:search`
    /// virtual module.
    #[cfg(feature = "js")]
    pub(crate) fn module(self, base: &str, index: &str) -> String {
        self.script(base, index).finish()
    }

    /// The sources every build of this format's client is assembled from, and
    /// the two constants they close over: `BASE`, prepended to each hit's href
    /// so a subdirectory-hosted site resolves it, and `INDEX`, the URL of the
    /// index this client fetches.
    ///
    /// The two are separate because they scope differently: hits carry
    /// already-localized permalinks, so folding the language into `BASE` would
    /// double it.
    fn script(self, base: &str, index: &str) -> Script<'static> {
        Script::new(&[("BASE", base), ("INDEX", index)])
            .part(TOKENIZE)
            .part(self.engine())
            .part(PALETTE)
    }

    /// The served URL of this format's index for `lang`: what the generated
    /// client fetches, and the single spelling shared by the emitted client and
    /// the `baudelaire:search` virtual module.
    pub(crate) fn index(self, config: &Config, lang: &str) -> String {
        let dir = config.prefixed(&Permalink::join(&[&config.scope(lang, "")]));
        format!("{dir}{}", self.file())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::emit::Output;

    /// The index key and the query key are computed in two languages, so nothing
    /// can execute both here. What is pinned instead is each side of the
    /// contract: the Rust normalization against the cases that broke, and the
    /// text of the client rule that has to mirror it.
    ///
    /// Every case below is one where the two tokenizers *disagreed*. A `\p{L}`
    /// client, or a strip-then-lowercase index, fails this test.
    #[test]
    fn tokens_agree_with_the_client_tokenizer() {
        // `İ` lowercases to `i` + U+0307 COMBINING DOT ABOVE. Both sides must
        // drop the mark the lowercasing introduced.
        assert_eq!(Document::normalize("İstanbul"), "istanbul");
        // Devanagari vowel signs are `Mc`/`Mn`, outside `\p{L}` but inside
        // `Alphabetic`, so they are kept. The virama is neither, so it goes:
        // what matters is that the client agrees, not which way it lands.
        assert_eq!(Document::normalize("हिन्दी"), "हिनदी");
        // Same class, Arabic and Hebrew points, all of them retained.
        assert_eq!(Document::normalize("مُحَمَّد"), "مُحَمَّد");
        assert_eq!(Document::normalize("שָׁלוֹם"), "שָׁלוֹם");
        // Precomposed accents survive; they are single alphabetic codepoints.
        assert_eq!(Document::normalize("ÅNGSTRÖM"), "ångström");
        // A titlecase digraph lowercases to one codepoint, not two letters.
        assert_eq!(Document::normalize("ǅungla"), "ǆungla");
        // Punctuation goes, including inside a word, so both sides join the
        // halves rather than one splitting them.
        assert_eq!(Document::normalize("foo-bar!"), "foobar");
        // `\p{N}` is wider than the digits: superscripts are numeric too.
        assert_eq!(Document::normalize("x²"), "x²");
        // Nothing survivable leaves an empty token, which `tokens` drops.
        assert_eq!(Document::normalize("--"), "");

        // The client rule, checked as text: these are the two edits that would
        // silently reintroduce the split, and neither is visible from Rust.
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
        let corpus = |outputs: &[Output]| {
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
        // "fast" appears in both docs; "rust" only in the first.
        assert_eq!(postings["fast"], serde_json::json!([0, 1]));
        assert_eq!(postings["rust"], serde_json::json!([0]));
        // Stopword "is" and sub-min-length tokens are excluded.
        assert!(postings.get("is").is_none(), "stopword dropped: {json}");
    }
}
