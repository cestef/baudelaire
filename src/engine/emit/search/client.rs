//! The generated JavaScript client: the engine a format needs, the shared
//! tokenizer, and the palette UI, assembled into one module scope.

use super::super::script::Script;
use crate::config::{Config, Permalink, SearchFormat};

/// The query tokenizer, shared by both engines and by the palette.
pub(super) const TOKENIZE: &str = include_str!("js/tokenize.js");

/// The self-mounting command-palette UI, concatenated onto whichever engine a
/// format needs.
const PALETTE: &str = include_str!("js/palette.js");

/// The generated client's entry point.
const MOUNT: &str = "mountSearch";

impl SearchFormat {
    /// The per-format engine source, defining `createSearch`.
    fn engine(self) -> &'static str {
        match self {
            Self::Json => include_str!("js/engine.flat.js"),
            Self::Inverted => include_str!("js/engine.inverted.js"),
        }
    }

    /// The standalone generated client: tokenizer, engine and palette UI, with
    /// an auto-mount.
    pub(super) fn client(self, base: &str, index: &str) -> String {
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
    use super::super::corpus::Document;

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
}
