//! `content { history }`: what a page learns about its own file from the
//! repository's log.
//!
//! These build real repositories, because the thing under test is what `git`
//! answers and not what a parser makes of a fixture.

use crate::common::{CONFIG, Site};

/// A site whose pages print their own `page.git`, in a repository with one
/// commit per page.
fn committed(config: &str) -> Site {
    let site = Site::with(config);
    site.git(&["init", "-q"]);
    site.write("templates/page.typ", TEMPLATE);
    site.write("content/a.typ", &page("A"));
    site.write("content/b.typ", &page("B"));
    site.git(&["add", "-A"]);
    site.git(&["commit", "-q", "-m", "both"]);
    site
}

/// A layout that prints everything `page.git` carries, so an assertion below
/// reads the page rather than a fixture.
const TEMPLATE: &str = r#"#let page(page, body) = {
  let git = page.at("git", default: none)
  let text = if git == none {
    "no history"
  } else {
    let credits = git.at("contributors", default: ())
    let names = credits.map(c => c.name + ":" + str(c.commits))
    let who = if names.len() == 0 { "" } else { names.join(",") }
    let parts = (
      "hash=" + git.hash,
      "when=" + git.committed,
      "by=" + git.author.name,
      "who=" + who,
    )
    parts.join(" ")
  }
  html.elem("main", text)
}
"#;

fn page(title: &str) -> String {
    format!("#let frontmatter = (title: \"{title}\", template: \"page.typ\",)\nbody\n")
}

/// The site config with history on, and `contributors` when asked for.
fn config(contributors: bool) -> String {
    let block = if contributors {
        "content {\n  history {\n    contributors #true\n  }\n}\n"
    } else {
        "content {\n  history\n}\n"
    };
    format!("{CONFIG}paths {{\n  templates \"templates\"\n}}\n{block}")
}

#[test]
fn a_page_carries_the_commit_that_last_changed_it() {
    let site = committed(&config(false));
    site.git(&["commit", "-q", "--allow-empty", "-m", "unrelated"]);
    site.write("content/a.typ", &format!("{}edited\n", page("A")));
    site.git(&["add", "-A"]);
    site.git(&["commit", "-q", "-m", "just a"]);
    site.stats();

    let a = site.output("a/index.html");
    let b = site.output("b/index.html");

    assert!(a.contains("by=t"), "the page names its author: {a}");
    assert!(
        !a.contains("no history") && !b.contains("no history"),
        "both pages are committed: {a} / {b}"
    );
    assert_ne!(
        hash(&a),
        hash(&b),
        "only `a` was touched by the last commit, so the two differ"
    );
}

/// A file the repository has never seen has no history, and that is not an
/// error: it is a page written since the last commit.
#[test]
fn an_uncommitted_page_has_no_history() {
    let site = committed(&config(false));
    site.write("content/fresh.typ", &page("Fresh"));
    site.stats();

    assert!(site.output("fresh/index.html").contains("no history"));
    assert!(!site.output("a/index.html").contains("no history"));
}

/// Nothing is read at all without the block, so a site that never asked pays
/// nothing and sees nothing.
#[test]
fn history_is_off_until_the_block_asks_for_it() {
    let site = committed(CONFIG_WITH_TEMPLATES);
    site.stats();

    assert!(site.output("a/index.html").contains("no history"));
}

const CONFIG_WITH_TEMPLATES: &str =
    "site \"T\"\npaths {\n  content \"content\"\n  dist \"public\"\n  templates \"templates\"\n}\n";

#[test]
fn contributors_are_gathered_only_when_they_are_asked_for() {
    let site = committed(&config(false));
    site.stats();
    assert!(
        !site.output("a/index.html").contains("who=t:"),
        "contributors are their own opt-in"
    );

    let site = committed(&config(true));
    site.write("content/a.typ", &format!("{}edited\n", page("A")));
    site.git(&["add", "-A"]);
    site.git_as("Ada", "ada@example.com", &["commit", "-q", "-m", "again"]);
    site.stats();

    let a = site.output("a/index.html");
    assert!(
        a.contains("who=Ada:1,t:1") || a.contains("who=t:1,Ada:1"),
        "both authors, one commit each: {a}"
    );
    assert!(
        site.output("b/index.html").contains("who=t:1"),
        "b was only ever touched once"
    );
}

/// A page committed after the sitemap's `lastmod` would have read its
/// frontmatter `date` reports the day it actually changed.
#[test]
fn the_sitemap_takes_lastmod_from_the_repository() {
    let site = Site::with(
        "site \"T\"\nurl \"https://example.com\"\n\
         paths {\n  content \"content\"\n  dist \"public\"\n}\n\
         generate {\n  sitemap #true\n}\n\
         content {\n  history\n}\n",
    );
    site.git(&["init", "-q"]);
    site.write(
        "content/a.typ",
        "#let frontmatter = (title: \"A\", date: datetime(year: 2020, month: 1, day: 2))\nbody\n",
    );
    site.git(&["add", "-A"]);
    site.git(&["commit", "-q", "-m", "one"]);
    site.stats();

    let sitemap = site.output("sitemap.xml");

    assert!(
        !sitemap.contains("<lastmod>2020-01-02</lastmod>"),
        "the publication date is not when the page last changed: {sitemap}"
    );
    assert!(
        sitemap.contains("<lastmod>"),
        "the commit date is: {sitemap}"
    );
}

/// An author who wrote `updated` has said when the page changed, and is
/// believed over the repository.
#[test]
fn an_explicit_updated_date_wins_over_the_repository() {
    let site = Site::with(
        "site \"T\"\nurl \"https://example.com\"\n\
         paths {\n  content \"content\"\n  dist \"public\"\n}\n\
         generate {\n  sitemap #true\n}\n\
         content {\n  history\n}\n",
    );
    site.git(&["init", "-q"]);
    site.write(
        "content/a.typ",
        "#let frontmatter = (title: \"A\", updated: datetime(year: 2021, month: 3, day: 4))\nbody\n",
    );
    site.git(&["add", "-A"]);
    site.git(&["commit", "-q", "-m", "one"]);
    site.stats();

    assert!(
        site.output("sitemap.xml")
            .contains("<lastmod>2021-03-04</lastmod>")
    );
}

/// The hash a page printed, for comparing two pages without asserting on a
/// value no test can know.
fn hash(page: &str) -> &str {
    let at = page.find("hash=").expect("the page prints its hash") + "hash=".len();
    let rest = &page[at..];
    &rest[..rest.find(' ').expect("a field follows the hash")]
}
