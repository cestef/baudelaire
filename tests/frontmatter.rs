use baudelaire::content::Frontmatter;
use typst::syntax::Source;

fn extract(text: &str) -> (Frontmatter, String) {
    let src = Source::detached(text);
    let e = Frontmatter::extract(&src)
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
    let src = Source::detached("just body");
    assert!(Frontmatter::extract(&src).expect("ok").is_none());
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
    let src = Source::detached(
        r#"
#frontmatter((title: ))
body
"#,
    );
    assert!(Frontmatter::extract(&src).is_err());
}

#[test]
fn non_dict_frontmatter_errors() {
    let src = Source::detached(
        r#"
#frontmatter("not a dict")
body
"#,
    );
    assert!(Frontmatter::extract(&src).is_err());
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
