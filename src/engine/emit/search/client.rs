//! The generated palette client: one file for the whole site, holding where
//! each language's index is served from and what the palette defaults to.

use super::super::script::Script;
use super::tokens::Tokens;
use crate::codegen::Value;
use crate::config::{Config, Permalink, SearchConfig};

/// The query tokenizer, shared by the engine and by the palette that
/// highlights what a query matched.
const TOKENIZE: &str = include_str!("js/tokenize.js");

/// The engine over either index shape, defining `createSearch`.
const ENGINE: &str = include_str!("js/engine.js");

/// The self-mounting command-palette UI.
const PALETTE: &str = include_str!("js/palette.js");

/// The generated client's entry point.
const MOUNT: &str = "mountSearch";

/// The one client every language shares, and the file names it and the indexes
/// are written under.
pub(crate) struct Client;

impl Client {
    pub(super) const INDEX: &'static str = "search.json";
    pub(super) const FILE: &'static str = "search.js";

    /// The standalone client, mounting itself: one `<script>` is a working
    /// search box.
    pub(super) fn standalone(config: &Config) -> String {
        Self::script(config).mount(MOUNT)
    }

    /// The composable module source served to bundlers through the
    /// `baudelaire:search` virtual module, with no auto-mount.
    #[cfg(feature = "js")]
    pub(crate) fn module(config: &Config) -> String {
        Self::script(config).finish()
    }

    /// The sources the client is assembled from and the constants they close
    /// over. Everything else a query needs travels in the index itself, so one
    /// client serves every language.
    fn script(config: &Config) -> Script<'static> {
        Script::data(&[
            ("KEPT", Value::str(Tokens::KEPT)),
            ("INDEXES", Self::indexes(config)),
            ("LANG", Value::str(&config.lang)),
            ("OPTIONS", Self::options(&config.generate.search)),
        ])
        .part(TOKENIZE)
        .part(ENGINE)
        .part(PALETTE)
    }

    /// Where each language's index is served from, which the client picks
    /// between by the page's own `lang`.
    fn indexes(config: &Config) -> Value {
        Value::dict(
            config
                .langs()
                .into_iter()
                .map(|lang| (lang.to_owned(), Value::str(Self::url(config, lang)))),
        )
    }

    /// The served URL of `lang`'s index.
    fn url(config: &Config, lang: &str) -> String {
        let dir = config.prefixed(&Permalink::join(&[&config.scope(lang, "")]));
        format!("{dir}{}", Self::INDEX)
    }

    /// The palette's configured defaults, which a `mountSearch` call overrides
    /// key by key.
    fn options(config: &SearchConfig) -> Value {
        Value::dict([
            ("hotkey", Value::str(&config.ui.hotkey)),
            ("placeholder", Value::str(&config.ui.placeholder)),
            (
                "limit",
                Value::Int(i64::try_from(config.ui.limit).unwrap_or(i64::MAX)),
            ),
            ("styles", Value::Bool(config.ui.styles)),
        ])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::emit::search::tokens::Tokens;

    /// Every case below is one where the two tokenizers *disagreed*: a `\p{L}`
    /// client, or a strip-then-lowercase index, fails this test.
    #[test]
    fn tokens_agree_with_the_client_tokenizer() {
        assert_eq!(Tokens::normalize("İstanbul"), "istanbul");
        assert_eq!(Tokens::normalize("हिन्दी"), "हिनदी");
        assert_eq!(Tokens::normalize("مُحَمَّد"), "مُحَمَّد");
        assert_eq!(Tokens::normalize("שָׁלוֹם"), "שָׁלוֹם");
        assert_eq!(Tokens::normalize("ÅNGSTRÖM"), "ångström");
        assert_eq!(Tokens::normalize("ǅungla"), "ǆungla");
        assert_eq!(Tokens::normalize("foo-bar!"), "foobar");
        assert_eq!(Tokens::normalize("x²"), "x²");
        assert_eq!(Tokens::normalize("--"), "");

        let client = Client::standalone(&Config::default());
        assert!(
            client.contains(r#"const KEPT = "\\p{Alphabetic}\\p{N}";"#),
            "the client strips by the class `Tokens::KEPT` states: {client}"
        );
        let (lower, strip) = (
            TOKENIZE.find("toLowerCase").expect("client lowercases"),
            TOKENIZE.find("replace").expect("client strips"),
        );
        assert!(
            lower < strip,
            "the client must lowercase before stripping, as `Tokens::normalize` does"
        );
    }

    #[test]
    fn every_language_index_is_reachable_from_the_one_client() {
        let mut config = Config::default();
        config.languages = vec![("fr".to_owned(), crate::config::LanguageConfig::default())];
        let script = Client::standalone(&config);
        assert!(script.contains(r#""en": "/search.json""#), "{script}");
        assert!(script.contains(r#""fr": "/fr/search.json""#), "{script}");
    }
}
