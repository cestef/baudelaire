//! `content { }`: collections, taxonomies, drafts, the bundle index.

use super::parse;
use crate::config::{Config, SortKey};
use crate::error::BaudelaireErrorKind;
use miette::Diagnostic;
#[test]
fn bundle_index_defaults_to_index_and_is_configurable() {
    assert_eq!(parse("").content.index.as_deref(), Some("index"));
    assert_eq!(
        parse("content {\n  index \"_index\"\n}")
            .content
            .index
            .as_deref(),
        Some("_index")
    );
    assert_eq!(parse("content {\n  index \"\"\n}").content.index, None);
}

#[test]
fn collections_overrides() {
    let cfg = parse(
        r#"
        content {
          collections {
            posts "posts/**/*.typ" {
              sort "date"
              reverse #true
              permalink "/posts/{slug}/"
            }
            notes "notes/**/*.typ" { sort "order" }
          }
        }
    "#,
    );
    let posts = cfg
        .content
        .collections
        .iter()
        .find(|(n, _)| n == "posts")
        .unwrap();
    assert_eq!(posts.1.glob.as_deref(), Some("posts/**/*.typ"));
    assert_eq!(posts.1.sort, SortKey::Date);
    assert!(posts.1.reverse);
    assert_eq!(posts.1.permalink.as_deref(), Some("/posts/{slug}/"));
    let notes = cfg
        .content
        .collections
        .iter()
        .find(|(n, _)| n == "notes")
        .unwrap();
    assert_eq!(notes.1.sort, SortKey::Order);
    assert!(!notes.1.reverse);
}

/// The named form is read after the positional, so a line writing both takes
/// the one it spelled out.
#[test]
fn a_collection_glob_can_be_named_as_well_as_positional() {
    let cfg = parse(r#"content { collections { posts { glob "p/**/*.typ" } } }"#);
    let posts = &cfg.content.collections[0].1;
    assert_eq!(posts.glob.as_deref(), Some("p/**/*.typ"));

    let cfg = parse(r#"content { collections { posts "a/*.typ" { glob "b/*.typ" } } }"#);
    assert_eq!(
        cfg.content.collections[0].1.glob.as_deref(),
        Some("b/*.typ")
    );
}

#[test]
fn taxonomies() {
    let cfg = parse(
        r#"
        content {
          taxonomies {
            tags   listing=#true
            series key="series" listing=#false
          }
        }
    "#,
    );
    let tags = cfg
        .content
        .taxonomies
        .iter()
        .find(|(n, _)| n == "tags")
        .unwrap();
    assert!(tags.1.listing);
    let series = cfg
        .content
        .taxonomies
        .iter()
        .find(|(n, _)| n == "series")
        .unwrap();
    assert_eq!(series.1.key, "series");
    assert!(!series.1.listing);
}

#[test]
fn err_bad_sort_key() {
    let err = Config::parse("content {\n  collections {\n    posts { sort \"wat\" }\n  }\n}\n")
        .unwrap_err();
    let rendered = format!("{:?}", miette::Report::from(err));
    assert!(rendered.contains("unknown value `wat`"), "{rendered}");
    assert!(rendered.contains("`order`, `date`, `title`"), "{rendered}");
}

#[test]
fn err_paginate_below_one() {
    for (config, detail) in [
        (
            "content {\n  collections {\n    posts { paginate { size 0 } }\n  }\n}\n",
            "paginate must be at least 1, got 0",
        ),
        (
            "content {\n  collections {\n    posts { paginate { size -3 } }\n  }\n}\n",
            "paginate must be at least 1, got -3",
        ),
    ] {
        let err = Config::parse(config).unwrap_err();
        assert!(err.to_string().contains(detail), "{err}");
    }
}

#[test]
fn err_duplicate_collection() {
    let err = Config::parse(
        "content {\n  collections {\n    posts\n    posts { sort \"date\" }\n  }\n}\n",
    )
    .unwrap_err();
    assert!(
        err.to_string().contains("duplicate collection `posts`"),
        "{err}"
    );
}

#[test]
fn err_duplicate_taxonomy() {
    let err =
        Config::parse("content {\n  taxonomies {\n    tags\n    tags listing=#true\n  }\n}\n")
            .unwrap_err();
    assert!(
        err.to_string().contains("duplicate taxonomy `tags`"),
        "{err}"
    );
}

/// `index` names a stem, matched against `Stem::slug`, which never carries an
/// extension: every page extension is refused, since `index.md` matched no page
/// and left the site with nothing at `/`.
#[test]
fn index_rejects_a_filename_and_names_the_stem() {
    for written in ["index.typ", "index.md"] {
        let err =
            Config::parse(&format!("content {{ index {written:?} }}")).expect_err("should refuse");
        let BaudelaireErrorKind::Config(config) = &err else {
            panic!("expected a config diagnostic, got: {err:?}");
        };
        assert_eq!(
            config.code().map(|c| c.to_string()).as_deref(),
            Some("baudelaire::config::index_extension"),
            "{written}"
        );
        let help = config.help().expect("a help").to_string();
        assert!(help.contains("index"), "help should name the stem: {help}");
    }

    assert_eq!(
        parse("content { index \"index\" }")
            .content
            .index
            .as_deref(),
        Some("index")
    );
    assert_eq!(parse("content { index \"\" }").content.index, None);
    assert_eq!(
        parse("content { index \"_index\" }")
            .content
            .index
            .as_deref(),
        Some("_index")
    );
}

/// Root pages reach `_root` through the collection they are discovered into.
#[test]
fn a_template_binding_resolves_nearest_first() {
    let config = parse(
        "content { collections { _root { template \"site.typ\" }; posts { template \"post.typ\" } } }",
    );
    assert_eq!(
        config
            .template_for("posts", Some("own.typ".into()))
            .as_deref(),
        Some("own.typ")
    );
    assert_eq!(
        config.template_for("posts", None).as_deref(),
        Some("post.typ")
    );
    assert_eq!(
        config.template_for(crate::content::ROOT, None).as_deref(),
        Some("site.typ")
    );
    assert_eq!(config.template_for("notes", None), None);
}

#[test]
fn drafts_takes_a_bare_flag_for_its_build_key() {
    assert!(!parse("").content.drafts.build, "off by default");
    assert!(parse("content { drafts #true }").content.drafts.build);
    assert!(parse("content { drafts }").content.drafts.build, "bare");
    assert!(!parse("content { drafts #false }").content.drafts.build);
    let cfg = parse("content { drafts #true }");
    assert_eq!(cfg.content.drafts.suffix, ".draft", "sibling untouched");
}

#[test]
fn drafts_still_takes_its_block_with_or_without_the_flag() {
    let cfg = parse("content { drafts { build #true; suffix \".wip\" } }");
    assert!(cfg.content.drafts.build);
    assert_eq!(cfg.content.drafts.suffix, ".wip");
    let cfg = parse("content { drafts #true { suffix \".wip\" } }");
    assert!(cfg.content.drafts.build);
    assert_eq!(cfg.content.drafts.suffix, ".wip");
}

/// The argument reaches the `build` handler untouched, so the shorthand cannot
/// accept a value `drafts { build .. }` would refuse.
#[test]
fn a_non_boolean_draft_shorthand_is_a_type_error() {
    let err = Config::parse("content { drafts \"yes\" }")
        .expect_err("string is not a boolean")
        .to_string();
    assert!(err.contains("expected boolean"), "{err}");
}

/// The rename from `draft` is a hard error rather than a silent no-op, an
/// ignored `draft { build #true }` being a build that drops every draft page.
#[test]
fn the_old_draft_spelling_is_refused_with_a_suggestion() {
    let err = Config::parse("content { draft { build #true } }").expect_err("renamed key");
    let rendered = format!("{:?}", miette::Report::from(err));
    assert!(rendered.contains("did you mean `drafts`?"), "{rendered}");
}

/// A key that is part of a URL is held to the permalink rule whichever way it
/// is written, a taxonomy's attribute as much as `paginate`'s node.
#[test]
fn a_taxonomy_prefix_is_a_permalink_piece_like_its_sibling() {
    for kdl in [
        "content {\n  taxonomies {\n    tags prefix=\"..\"\n  }\n}",
        "content {\n  collections {\n    posts {\n      paginate {\n        prefix \"..\"\n      }\n    }\n  }\n}",
    ] {
        let err = Config::parse(kdl)
            .expect_err("`..` escapes the output directory")
            .to_string();
        assert!(err.contains(".."), "{kdl}: {err}");
    }
    let config = Config::parse("content {\n  taxonomies {\n    tags prefix=\"seite\"\n  }\n}")
        .expect("an ordinary prefix");
    assert_eq!(config.content.taxonomies[0].1.prefix, "seite");
}
