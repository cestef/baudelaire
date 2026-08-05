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
    // an empty basename disables bundle slugs
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

/// `glob` is the field's name in the docs and in the struct, and it is now the
/// parser's too. It used to be positional-only, so writing the documented thing
/// failed with a help that listed every key except the one being written.
#[test]
fn a_collection_glob_can_be_named_as_well_as_positional() {
    let cfg = parse(r#"content { collections { posts { glob "p/**/*.typ" } } }"#);
    let posts = &cfg.content.collections[0].1;
    assert_eq!(posts.glob.as_deref(), Some("p/**/*.typ"));

    // The named form is read after the positional, so a line writing both takes
    // the one it spelled out.
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
    // an unknown enum *value* reads as "unknown value", not "unknown key"
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
/// extension. The docs shipped `index "index.typ"`, which matches no page: every
/// bundle keeps its filename slug, `content/index.typ` publishes to `/index/`,
/// and the build reports success with nothing at `/`.
#[test]
fn index_rejects_a_filename_and_names_the_stem() {
    let err = Config::parse("content { index \"index.typ\" }").expect_err("should refuse");
    let BaudelaireErrorKind::Config(config) = &err else {
        panic!("expected a config diagnostic, got: {err:?}");
    };
    assert_eq!(
        config.code().map(|c| c.to_string()).as_deref(),
        Some("baudelaire::config::index_extension")
    );
    // The help has to carry the correction, not just the complaint.
    let help = config.help().expect("a help").to_string();
    assert!(help.contains("index"), "help should name the stem: {help}");

    // The stem itself is what the key takes...
    assert_eq!(
        parse("content { index \"index\" }")
            .content
            .index
            .as_deref(),
        Some("index")
    );
    // ...and the empty string stays the documented way to turn bundles off.
    assert_eq!(parse("content { index \"\" }").content.index, None);
    // A stem that merely contains a dot is not a filename, and is left alone.
    assert_eq!(
        parse("content { index \"_index\" }")
            .content
            .index
            .as_deref(),
        Some("_index")
    );
}

/// The two places a layout can be named, nearer first, and the root pages that
/// reach the second through the collection they are discovered into: a theme
/// binds `_root` and a page that names nothing still renders through it.
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
    // A collection nothing configures binds nothing: the page renders unwrapped.
    assert_eq!(config.template_for("notes", None), None);
}

/// The whole point of the shorthand: the one-boolean case, which is what a
/// `dev` profile writes, does not have to open a block to say it, and every
/// sibling key of the section it stands in for is left alone.
#[test]
fn drafts_takes_a_bare_flag_for_its_build_key() {
    assert!(!parse("").content.drafts.build, "off by default");
    assert!(parse("content { drafts #true }").content.drafts.build);
    assert!(parse("content { drafts }").content.drafts.build, "bare");
    assert!(!parse("content { drafts #false }").content.drafts.build);
    let cfg = parse("content { drafts #true }");
    assert_eq!(cfg.content.drafts.suffix, ".draft", "sibling untouched");
}

/// The long spelling still reads, and an argument may carry a block: the
/// shorthand is an extra spelling of one key, not a replacement for the block.
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

/// The key was `draft` through 0.0.11, and the rename is a hard error rather
/// than a silent no-op: an ignored `draft { build #true }` is a production
/// build that quietly drops every draft page.
#[test]
fn the_old_draft_spelling_is_refused_with_a_suggestion() {
    let err = Config::parse("content { draft { build #true } }").expect_err("renamed key");
    let rendered = format!("{:?}", miette::Report::from(err));
    assert!(rendered.contains("did you mean `drafts`?"), "{rendered}");
}
