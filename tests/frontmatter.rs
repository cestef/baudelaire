mod common;

use baudelaire::content::{Frontmatter, Page};
use common::Site;

/// Load `text` as a page in a site declaring the `tags` and `series`
/// taxonomies.
fn try_load(text: &str) -> baudelaire::error::Result<Page> {
    try_load_with(
        text,
        "site \"T\"\ncontent {\n  taxonomies {\n    tags\n    series\n  }\n}\n",
    )
}

fn try_load_with(text: &str, config: &str) -> baudelaire::error::Result<Page> {
    let site = Site::new();
    site.write("config.kdl", config);
    site.write("content/posts/page.typ", text);
    let cfg = site.config();
    common::load_page("posts", &site.root.join("content/posts/page.typ"), &cfg)
}

fn extract(text: &str) -> Frontmatter {
    try_load(text).expect("load").frontmatter
}

#[test]
fn extracts_scalar_fields() {
    let fm = extract(
        r#"
#let frontmatter = (
  title: "Hello World",
  draft: false,
  slug: "hello-world",
  template: "post.typ",
  order: 3,
)
#html.frame[Body]
"#,
    );
    assert_eq!(fm.title.as_deref(), Some("Hello World"));
    assert!(!fm.draft);
    assert_eq!(fm.slug.as_deref(), Some("hello-world"));
    assert_eq!(fm.template.as_deref(), Some("post.typ"));
    assert_eq!(fm.order, Some(3));
}

#[test]
fn extracts_date() {
    let fm = extract(
        r"
#let frontmatter = (
  date: datetime(year: 2024, month: 1, day: 15),
)
body
",
    );
    let date = fm.date.expect("date");
    assert_eq!(date.year(), 2024);
    assert_eq!(date.month(), time::Month::January);
    assert_eq!(date.day(), 15);
}

#[test]
fn extracts_taxonomy_lists() {
    let fm = extract(
        r#"
#let frontmatter = (
  tags: ("intro", "typst"),
  series: ("build",),
)
body
"#,
    );
    assert_eq!(
        fm.taxonomies.get("tags").unwrap(),
        &vec!["intro".to_owned(), "typst".to_owned()]
    );
    assert_eq!(
        fm.taxonomies.get("series").unwrap(),
        &vec!["build".to_owned()]
    );
}

#[test]
fn extracts_redirect_list() {
    let fm = extract(
        r#"
#let frontmatter = (
  redirect: ("/old", "/older"),
)
body
"#,
    );
    assert_eq!(fm.redirect, vec!["/old".to_owned(), "/older".to_owned()]);
}

#[test]
fn extra_keys_passed_through() {
    let fm = extract(
        r#"
#let frontmatter = (
  title: "X",
  custom: "hello",
  count: 42,
)
body
"#,
    );
    assert_eq!(fm.extra.len(), 2);
    assert!(fm.extra.contains_key("custom"));
    assert!(fm.extra.contains_key("count"));
}

#[test]
fn frontmatter_is_computed_like_any_export() {
    let fm = extract(
        r#"
#let series = "build"
#let frontmatter = (
  title: "Part 2 - " + series,
  series: (series,),
)
body
"#,
    );
    assert_eq!(fm.title.as_deref(), Some("Part 2 - build"));
    assert_eq!(
        fm.taxonomies.get("series").unwrap(),
        &vec!["build".to_owned()]
    );
}

#[test]
fn no_frontmatter_returns_defaults() {
    let page = try_load("just body").expect("load");
    assert!(page.frontmatter.title.is_none());
    assert!(!page.frontmatter.draft);
}

#[test]
fn empty_frontmatter_defaults() {
    let fm = extract("#let frontmatter = (:)\nbody\n");
    assert_eq!(fm.title, None);
    assert!(!fm.draft);
    assert!(fm.taxonomies.is_empty());
}

#[test]
fn malformed_frontmatter_errors() {
    let err = try_load("\n#let frontmatter = (title: \"unterminated)\nbody\n").unwrap_err();
    let rendered = format!("{:?}", miette::Report::new(err));
    assert!(rendered.contains("page.typ"), "{rendered}");
    assert!(
        rendered.contains("╭─"),
        "expected a source snippet: {rendered}"
    );
}

#[test]
fn non_dict_frontmatter_errors() {
    let err = try_load("\n#let frontmatter = \"not a dict\"\nbody\n").unwrap_err();
    assert!(err.to_string().contains("dictionary"), "{err}");
}

#[test]
fn legacy_call_form_is_a_migration_error() {
    let err = try_load("#frontmatter((title: \"X\"))\nbody\n").unwrap_err();
    let rendered = format!("{:?}", miette::Report::new(err));
    assert!(rendered.contains("#let frontmatter = "), "{rendered}");
}

/// KDL has no date literal, so a markdown page writes its date as an ISO
/// string, and one reader serves both dialects.
#[test]
fn an_iso_string_reads_as_a_date() {
    let page = try_load("#let frontmatter = (date: \"2024-01-01\")").expect("a valid date");
    assert_eq!(
        page.frontmatter.date.map(|d| d.to_string()),
        Some("2024-01-01".to_owned())
    );
}

#[test]
fn wrong_typed_known_keys_error() {
    for bad in [
        "#let frontmatter = (title: 3)",
        "#let frontmatter = (draft: \"yes\")",
        // A string date is read as an ISO day: the day is wrong, not the type.
        "#let frontmatter = (date: \"the first of January\")",
        "#let frontmatter = (date: \"2024-1-1\")",
        "#let frontmatter = (order: \"first\")",
        "#let frontmatter = (tags: \"solo\")",
    ] {
        let err = try_load(bad).unwrap_err();
        assert!(
            err.to_string().contains("must be"),
            "expected a type error for `{bad}`, got: {err}"
        );
    }
}

#[test]
fn typo_key_is_suggested() {
    let err = try_load("#let frontmatter = (titel: \"X\")").unwrap_err();
    let rendered = format!("{:?}", miette::Report::new(err));
    assert!(rendered.contains("did you mean `title`"), "{rendered}");
}

#[test]
fn configured_taxonomy_key_is_recognized() {
    let page = try_load_with(
        "#let frontmatter = (categories: (\"rust\", \"cli\"))",
        "site \"T\"\ncontent {\n  taxonomies {\n    categories\n  }\n}\n",
    )
    .expect("load");
    assert_eq!(
        page.frontmatter.taxonomies.get("categories").unwrap(),
        &vec!["rust".to_owned(), "cli".to_owned()]
    );
}

#[test]
fn export_anywhere_in_the_module_counts() {
    let fm = extract("Some intro.\n\n#let frontmatter = (title: \"X\")\n");
    assert_eq!(fm.title.as_deref(), Some("X"));
}

/// The message reads `must be {expected}, but is {got}`, and the article
/// belongs to the noun.
#[test]
fn a_type_mismatch_reads_as_one_sentence() {
    for (frontmatter, want) in [
        (
            "(title: \"P\", date: \"yesterday\")",
            "must be a date, but is a string that is not an ISO day",
        ),
        ("(title: 3)", "must be a string, but is an integer"),
        (
            "(title: \"P\", draft: 7)",
            "must be a boolean, but is an integer",
        ),
    ] {
        let site = Site::with("site \"A\"\n");
        site.write(
            "content/a.typ",
            &format!("#let frontmatter = {frontmatter}\nx\n"),
        );
        let err = site.build_error().to_string();
        assert!(err.contains(want), "wanted `{want}`, got: {err}");
    }
}
