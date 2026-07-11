use std::path::Path;

use baudelaire::config::Config;
use baudelaire::content::Frontmatter;
use typst::syntax::Source;

/// A config declaring the `tags` and `series` taxonomies most fixtures use —
/// taxonomy keys are recognized from config, not hard-coded.
fn config() -> Config {
    Config::parse("taxonomies {\n  tags\n  series\n}\n").expect("config")
}

fn try_extract(text: &str, config: &Config) -> baudelaire::error::Result<Option<Frontmatter>> {
    let src = Source::detached(text);
    Ok(Frontmatter::extract(&src, Path::new("page.typ"), config)?.map(|e| e.frontmatter))
}

fn extract(text: &str) -> (Frontmatter, String) {
    let src = Source::detached(text);
    let e = Frontmatter::extract(&src, Path::new("page.typ"), &config())
        .expect("extract")
        .expect("has frontmatter");
    (e.frontmatter, e.body)
}

#[test]
fn extracts_scalar_fields() {
    let (fm, _) = extract(
        r#"
#frontmatter((
  title: "Hello World",
  draft: false,
  slug: "hello-world",
  template: "post.typ",
  order: 3,
))
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
    let (fm, _) = extract(
        r#"
#frontmatter((
  date: datetime(year: 2024, month: 1, day: 15),
))
body
"#,
    );
    let date = fm.date.expect("date");
    assert_eq!(date.year(), 2024);
    assert_eq!(date.month(), time::Month::January);
    assert_eq!(date.day(), 15);
}

#[test]
fn extracts_taxonomy_lists() {
    let (fm, _) = extract(
        r#"
#frontmatter((
  tags: ("intro", "typst"),
  series: ("build",),
))
body
"#,
    );
    assert_eq!(
        fm.taxonomies.get("tags").unwrap(),
        &vec!["intro".to_string(), "typst".to_string()]
    );
    assert_eq!(
        fm.taxonomies.get("series").unwrap(),
        &vec!["build".to_string()]
    );
}

#[test]
fn extracts_redirect_list() {
    let (fm, _) = extract(
        r#"
#frontmatter((
  redirect: ("/old", "/older"),
))
body
"#,
    );
    assert_eq!(fm.redirect, vec!["/old".to_string(), "/older".to_string()]);
}

#[test]
fn extra_keys_passed_through() {
    let (fm, _) = extract(
        r#"
#frontmatter((
  title: "X",
  custom: "hello",
  count: 42,
))
body
"#,
    );
    assert_eq!(fm.extra.len(), 2);
    assert!(fm.extra.contains_key("custom"));
    assert!(fm.extra.contains_key("count"));
}

#[test]
fn splices_out_frontmatter_call() {
    let (_, spliced) = extract(
        r#"
#frontmatter((
  title: "X",
))
#html.frame[Body]
"#,
    );
    assert!(!spliced.contains("frontmatter"));
    assert!(spliced.contains("Body"));
}

#[test]
fn no_frontmatter_returns_none() {
    assert!(try_extract("just body", &config()).expect("ok").is_none());
}

#[test]
fn empty_frontmatter_defaults() {
    let (fm, _) = extract(
        r#"
#frontmatter((:))
body
"#,
    );
    assert_eq!(fm.title, None);
    assert!(!fm.draft);
    assert!(fm.taxonomies.is_empty());
}

#[test]
fn malformed_frontmatter_errors() {
    let err = try_extract("\n#frontmatter((title: \"unterminated))\nbody\n", &config()).unwrap_err();
    // The error names the offending file, not just "failed to evaluate".
    assert!(err.to_string().contains("page.typ"), "{err}");
    // ...and the related diagnostic underlines the offending text (a code frame
    // renders), rather than a bare spanless message.
    let rendered = format!("{:?}", miette::Report::new(err));
    assert!(rendered.contains("╭─"), "expected a source snippet: {rendered}");
}

#[test]
fn non_dict_frontmatter_errors() {
    assert!(try_extract("\n#frontmatter(\"not a dict\")\nbody\n", &config()).is_err());
}

#[test]
fn body_after_frontmatter_preserved() {
    let (_, spliced) = extract(
        r#"
#frontmatter((title: "X"))
First line
Second line
"#,
    );
    assert!(spliced.contains("First line"));
    assert!(spliced.contains("Second line"));
}

#[test]
fn wrong_typed_known_keys_error() {
    // Previously these were silently dropped (`title: 3` vanished, a string
    // `draft`/`date` became false/none). Now each is a precise error.
    for bad in [
        "#frontmatter((title: 3))",
        "#frontmatter((draft: \"yes\"))",
        "#frontmatter((date: \"2024-01-01\"))",
        "#frontmatter((order: \"first\"))",
        "#frontmatter((tags: \"solo\"))",
    ] {
        let err = try_extract(bad, &config()).unwrap_err();
        assert!(
            err.to_string().contains("must be"),
            "expected a type error for `{bad}`, got: {err}"
        );
    }
}

#[test]
fn typo_key_is_suggested() {
    let err = try_extract("#frontmatter((titel: \"X\"))", &config()).unwrap_err();
    let rendered = format!("{:?}", miette::Report::new(err));
    assert!(rendered.contains("did you mean `title`"), "{rendered}");
}

#[test]
fn configured_taxonomy_key_is_recognized() {
    // A taxonomy keyed on a non-default name must populate `taxonomies`, not
    // silently land in `extra`.
    let config = Config::parse("taxonomies {\n  categories\n}\n").expect("config");
    let fm = try_extract("#frontmatter((categories: (\"rust\", \"cli\")))", &config)
        .expect("ok")
        .expect("frontmatter");
    assert_eq!(
        fm.taxonomies.get("categories").unwrap(),
        &vec!["rust".to_string(), "cli".to_string()]
    );
}

#[test]
fn mid_document_frontmatter_is_not_extracted() {
    // A `#frontmatter(...)` that is not the leading expression is ordinary
    // content, left in the body.
    let none = try_extract("Some intro.\n\n#frontmatter((title: \"X\"))\n", &config()).expect("ok");
    assert!(none.is_none());
}

#[test]
fn splice_preserves_line_numbers() {
    // A three-line frontmatter call leaves the body's first line at its
    // original line number (blank lines stand in for the removed call).
    let (_, spliced) = extract("#frontmatter((\n  title: \"X\",\n))\nBody line\n");
    let body_line = spliced.lines().position(|l| l.contains("Body line")).expect("body");
    assert_eq!(body_line, 3, "body should stay on line 4 (index 3): {spliced:?}");
}
