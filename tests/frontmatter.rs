mod common;

use baudelaire::content::{Frontmatter, Page};
use baudelaire::world::{Mode, Project};
use common::Site;

/// Load `text` as a page in a site declaring the `tags` and `series`
/// taxonomies (taxonomy keys are recognized from config, not hard-coded) —
/// frontmatter is the page module's `frontmatter` export, so extraction goes
/// through real module evaluation.
fn try_load(text: &str) -> baudelaire::error::Result<Page> {
    try_load_with(text, "site \"T\"\ntaxonomies {\n  tags\n  series\n}\n")
}

fn try_load_with(text: &str, config: &str) -> baudelaire::error::Result<Page> {
    let site = Site::new();
    site.write("config.kdl", config);
    site.write("content/posts/page.typ", text);
    let cfg = site.config();
    let project = Project::new(&cfg, Mode::Build)?;
    Page::load(
        "posts",
        &site.root.join("content/posts/page.typ"),
        &cfg,
        &project,
    )
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
        r#"
#let frontmatter = (
  date: datetime(year: 2024, month: 1, day: 15),
)
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
        &vec!["intro".to_string(), "typst".to_string()]
    );
    assert_eq!(
        fm.taxonomies.get("series").unwrap(),
        &vec!["build".to_string()]
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
    assert_eq!(fm.redirect, vec!["/old".to_string(), "/older".to_string()]);
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
    // The export is evaluated by typst itself — it can use bindings, string
    // ops, whatever the language offers, not just a literal dict.
    let fm = extract(
        r#"
#let series = "build"
#let frontmatter = (
  title: "Part 2 — " + series,
  series: (series,),
)
body
"#,
    );
    assert_eq!(fm.title.as_deref(), Some("Part 2 — build"));
    assert_eq!(
        fm.taxonomies.get("series").unwrap(),
        &vec!["build".to_string()]
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
    // The eval diagnostics carry real spans: the report renders a code frame
    // against the page file, not a bare spanless message.
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
    // The pre-export `#frontmatter(...)` call is a precise error pointing at
    // the binding syntax — never a generic "unknown variable".
    let err = try_load("#frontmatter((title: \"X\"))\nbody\n").unwrap_err();
    let rendered = format!("{:?}", miette::Report::new(err));
    assert!(rendered.contains("#let frontmatter = "), "{rendered}");
}

#[test]
fn wrong_typed_known_keys_error() {
    // A known key with a wrong-typed value is a precise error, never silently
    // dropped (`title: 3` vanishing, a string `draft` becoming false).
    for bad in [
        "#let frontmatter = (title: 3)",
        "#let frontmatter = (draft: \"yes\")",
        "#let frontmatter = (date: \"2024-01-01\")",
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
    // A taxonomy keyed on a non-default name must populate `taxonomies`, not
    // silently land in `extra`.
    let page = try_load_with(
        "#let frontmatter = (categories: (\"rust\", \"cli\"))",
        "site \"T\"\ntaxonomies {\n  categories\n}\n",
    )
    .expect("load");
    assert_eq!(
        page.frontmatter.taxonomies.get("categories").unwrap(),
        &vec!["rust".to_string(), "cli".to_string()]
    );
}

#[test]
fn export_anywhere_in_the_module_counts() {
    // Real module semantics: any top-level `#let frontmatter` is the page's
    // export, wherever it appears in the file.
    let fm = extract("Some intro.\n\n#let frontmatter = (title: \"X\")\n");
    assert_eq!(fm.title.as_deref(), Some("X"));
}
