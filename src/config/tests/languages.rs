//! `languages { }` and everything keyed by a language code.

use super::{code, parse};
#[test]
fn languages_block_parses_with_names_and_strings() {
    let cfg = parse(
        r#"
        lang "en"
        languages {
            fr { name "Français"; strings { more "Lire la suite" } }
            de { name "Deutsch"; dir "ltr" }
        }
    "#,
    );
    assert_eq!(cfg.languages.len(), 2);
    let (code, fr) = &cfg.languages[0];
    assert_eq!(code, "fr");
    assert_eq!(fr.name.as_deref(), Some("Français"));
    assert_eq!(fr.strings.len(), 1);
}

#[test]
fn langs_lists_default_first_then_declared() {
    let cfg = parse("lang \"en\"\nlanguages {\n  fr { }\n  de { }\n}\n");
    assert_eq!(cfg.langs(), ["en", "fr", "de"]);
    assert!(cfg.knows("fr") && cfg.knows("en") && !cfg.knows("es"));
    assert!(cfg.multilingual());
}

#[test]
fn localize_prefixes_only_non_default_languages() {
    let cfg = parse("lang \"en\"\nlanguages {\n  fr { }\n}\n");
    assert_eq!(cfg.localize("en", "/posts/hello/"), "/posts/hello/");
    assert_eq!(cfg.localize("fr", "/posts/hello/"), "/fr/posts/hello/");
    assert_eq!(cfg.localize("fr", "/"), "/fr/");
}

/// Values carrying a `codegen::Value` reach the cache fingerprint through a
/// real `Hash`. Hashing their serialization instead meant any two configs whose
/// serialization failed fingerprinted identically, and a stale site.
#[test]
fn client_and_language_values_key_the_fingerprint() {
    use crate::graph::Hash;
    let base = parse("client {\n  env \"prod\"\n}\nlanguages {\n  fr { name \"Français\" }\n}");
    let client = parse("client {\n  env \"dev\"\n}\nlanguages {\n  fr { name \"Français\" }\n}");
    let language = parse("client {\n  env \"prod\"\n}\nlanguages {\n  fr { name \"Francais\" }\n}");
    assert_ne!(Hash::of(&base), Hash::of(&client));
    assert_ne!(Hash::of(&base), Hash::of(&language));
}

/// A monolingual right-to-left site has no `languages` block to declare `dir`
/// in, so the direction has to come from the code itself.
#[test]
fn rtl_is_inferred_when_no_language_declares_it() {
    assert_eq!(parse("lang \"ar\"").dir("ar"), Some("rtl"));
    assert_eq!(parse("lang \"az-Arab\"").dir("az-Arab"), Some("rtl"));
    assert_eq!(parse("lang \"en\"").dir("en"), None);
    assert_eq!(parse("lang \"fr\"").dir("fr"), None);
}

/// An explicit `dir` still wins over the inference.
#[test]
fn a_declared_dir_overrides_the_inferred_one() {
    let cfg = parse("lang \"en\"\nlanguages {\n  ar { dir \"ltr\" }\n}");
    assert_eq!(cfg.dir("ar"), Some("ltr"));
}

/// The reading rate is language-shaped: 200 words a minute is European prose,
/// and a page in a language read three times faster reported a third of the read
/// it is. A language states its own; the site states the answer for the rest.
#[test]
fn a_language_states_its_own_reading_rate() {
    let cfg = parse(
        r#"
        lang "en"
        content { reading { wpm 250 } }
        languages {
          en { name "English" }
          ja { name "日本語"; wpm 600 }
        }
    "#,
    );
    assert_eq!(cfg.wpm("ja"), 600);
    assert_eq!(
        cfg.wpm("en"),
        250,
        "the site's answer for a language with none"
    );
    assert_eq!(cfg.wpm("de"), 250, "and for one it does not declare at all");
    assert_eq!(crate::config::Config::default().wpm("en"), 200);
}

/// A rate of nothing is a division by zero dressed as a setting.
#[test]
fn err_a_reading_rate_of_none_is_refused() {
    for text in [
        "content { reading { wpm 0 } }",
        "languages { ja { wpm 0 } }",
    ] {
        assert_eq!(code(text), "baudelaire::config::out_of_range", "{text}");
    }
}
