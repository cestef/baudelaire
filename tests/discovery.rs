mod common;

use std::fs;

use baudelaire::content::{Page, discover};
use baudelaire::world::{Mode, Project};
/// A [`Project`] for a test config — module evaluation needs the real world.
fn project(cfg: &baudelaire::config::Config) -> Project {
    Project::new(cfg, Mode::Build).expect("project")
}

use common::Site;

fn frontmatter_post(title: &str, slug: &str, date: &str, tags: &[&str]) -> String {
    let tags_str = tags
        .iter()
        .map(|t| format!("\"{t}\""))
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        r#"#let frontmatter = (
  title: "{title}",
  slug: "{slug}",
  date: datetime({date}),
  tags: ({tags_str}),
)
#html.frame[Body of {title}]
"#
    )
}

#[test]
fn collection_sort_and_reverse_applied() {
    let site = Site::new();
    site.write(
        "config.kdl",
        r#"
            site "T"
            paths {
                content "content"
                dist "public"
            }
            collections {
              posts sort="date" reverse=#true
            }
        "#,
    );
    site.write(
        "content/posts/old.typ",
        &frontmatter_post("Old", "old", "year: 2020, month: 1, day: 1", &[]),
    );
    site.write(
        "content/posts/new.typ",
        &frontmatter_post("New", "new", "year: 2024, month: 1, day: 1", &[]),
    );
    let cols = discover(&site.config(), &project(&site.config())).unwrap();
    let posts = cols.iter().find(|c| c.id == "posts").unwrap();
    // date descending → newest first.
    let slugs: Vec<_> = posts
        .pages
        .iter()
        .map(|p| p.frontmatter.slug.as_deref().unwrap())
        .collect();
    assert_eq!(slugs, ["new", "old"]);
}

#[test]
fn discovers_collections_and_pages() {
    let site = Site::new();
    site.write(
        "config.kdl",
        r#"
            site "Test"
            paths {
                content "content"
                dist "public"
            }
            clean #true
            collections {
              posts "posts/**/*.typ" sort="date"
            }
        "#,
    );
    site.write(
        "content/posts/hello.typ",
        &frontmatter_post("Hello", "hello", "year: 2024, month: 1, day: 1", &["intro"]),
    );
    site.write(
        "content/posts/world.typ",
        &frontmatter_post("World", "world", "year: 2024, month: 2, day: 1", &["intro"]),
    );
    site.write(
        "content/notes/a.typ",
        "#let frontmatter = (title: \"A\",)\nbody a",
    );

    let cfg = site.config();
    let collections = discover(&cfg, &project(&cfg)).unwrap();
    let ids: Vec<&str> = collections.iter().map(|c| c.id.as_str()).collect();
    assert!(ids.contains(&"posts"));
    assert!(ids.contains(&"notes"));
    let posts = collections.iter().find(|c| c.id == "posts").unwrap();
    assert_eq!(posts.pages.len(), 2);
}

#[test]
fn glob_assigns_files_to_its_collection_regardless_of_directory() {
    let site = Site::new();
    site.write(
        "config.kdl",
        r#"
            site "T"
            paths {
                content "content"
                dist "public"
            }
            collections {
              blog "articles/**/*.typ"
            }
        "#,
    );
    // Files live under `articles/` but the glob routes them to `blog`.
    site.write(
        "content/articles/a.typ",
        "#let frontmatter = (title: \"A\",)\na",
    );
    site.write(
        "content/articles/sub/b.typ",
        "#let frontmatter = (title: \"B\",)\nb",
    );
    let cols = discover(&site.config(), &project(&site.config())).unwrap();
    let ids: Vec<&str> = cols.iter().map(|c| c.id.as_str()).collect();
    assert!(ids.contains(&"blog"), "glob collection missing: {ids:?}");
    assert!(
        !ids.contains(&"articles"),
        "files leaked to convention: {ids:?}"
    );
    let blog = cols.iter().find(|c| c.id == "blog").unwrap();
    assert_eq!(blog.pages.len(), 2);
}

#[test]
fn invalid_glob_reports_error() {
    let site = Site::new();
    site.write(
        "config.kdl",
        "site \"T\"\npaths {\n  content \"content\"\n}\ncollections {\n  x \"a/{unclosed\"\n}\n",
    );
    site.write("content/a/p.typ", "#let frontmatter = (title: \"P\",)\np");
    let err = discover(&site.config(), &project(&site.config())).unwrap_err();
    assert!(
        err.to_string().contains("glob"),
        "expected a glob error: {err}"
    );
}

#[test]
fn page_loads_frontmatter_and_body() {
    let site = Site::new();
    site.write("config.kdl", "site \"T\"\nclean #true");
    site.write(
        "content/posts/hello.typ",
        &frontmatter_post("Hello", "hello", "year: 2024, month: 1, day: 1", &[]),
    );
    let cfg = site.config();
    let path = site.root.join("content/posts/hello.typ");
    let page = Page::load("posts", &path, &cfg, &project(&cfg)).unwrap();
    assert_eq!(page.frontmatter.title.as_deref(), Some("Hello"));
    assert_eq!(page.frontmatter.slug.as_deref(), Some("hello"));
    assert!(page.body.contains("Body of Hello"));
    // the export stays in the body — it is a valid binding producing no
    // output, and lets the page read its own metadata.
    assert!(page.body.contains("#let frontmatter"));
}

#[test]
fn page_without_frontmatter_loads() {
    let site = Site::new();
    site.write("config.kdl", "site \"T\"");
    site.write("content/notes/plain.typ", "just body text");
    let cfg = site.config();
    let path = site.root.join("content/notes/plain.typ");
    let page = Page::load("notes", &path, &cfg, &project(&cfg)).unwrap();
    assert!(page.frontmatter.title.is_none());
    assert_eq!(page.body, "just body text");
    assert_eq!(page.id.0, "notes/plain");
}

#[test]
fn clean_urls_output_path() {
    let site = Site::new();
    site.write(
        "config.kdl",
        "site \"T\"\nclean #true\npaths {\n  dist \"public\"\n}",
    );
    site.write(
        "content/posts/hello.typ",
        &frontmatter_post("Hello", "hello", "year: 2024, month: 1, day: 1", &[]),
    );
    let cfg = site.config();
    let path = site.root.join("content/posts/hello.typ");
    let page = Page::load("posts", &path, &cfg, &project(&cfg)).unwrap();
    assert_eq!(page.output, site.root.join("public/posts/hello/index.html"));
}

#[test]
fn flat_urls_output_path() {
    let site = Site::new();
    site.write(
        "config.kdl",
        "site \"T\"\nclean #false\npaths {\n  dist \"public\"\n}",
    );
    site.write(
        "content/posts/hello.typ",
        &frontmatter_post("Hello", "hello", "year: 2024, month: 1, day: 1", &[]),
    );
    let cfg = site.config();
    let path = site.root.join("content/posts/hello.typ");
    let page = Page::load("posts", &path, &cfg, &project(&cfg)).unwrap();
    assert_eq!(page.output, site.root.join("public/posts/hello.html"));
}

#[test]
fn custom_permalink_template() {
    let site = Site::new();
    site.write(
        "config.kdl",
        r#"
            site "T"
            clean #true
            paths {
                dist "public"
            }
            collections {
              posts permalink="/posts/{year}/{slug}/"
            }
        "#,
    );
    site.write(
        "content/posts/hello.typ",
        &frontmatter_post("Hello", "hello", "year: 2024, month: 1, day: 1", &[]),
    );
    let cfg = site.config();
    let path = site.root.join("content/posts/hello.typ");
    let page = Page::load("posts", &path, &cfg, &project(&cfg)).unwrap();
    assert_eq!(page.permalink, "/posts/2024/hello/");
    assert_eq!(
        page.output,
        site.root.join("public/posts/2024/hello/index.html")
    );
}

#[test]
fn slug_falls_back_to_filename() {
    let site = Site::new();
    site.write("config.kdl", "site \"T\"\nclean #true");
    site.write(
        "content/posts/my-post.typ",
        "#let frontmatter = (title: \"X\",)\nbody",
    );
    let cfg = site.config();
    let path = site.root.join("content/posts/my-post.typ");
    let page = Page::load("posts", &path, &cfg, &project(&cfg)).unwrap();
    assert_eq!(page.id.0, "posts/my-post");
    assert_eq!(page.permalink, "/posts/my-post/");
}

#[test]
fn draft_page_skipped_unless_flag() {
    let site = Site::new();
    site.write("config.kdl", "site \"T\"");
    site.write(
        "content/posts/d.typ",
        "#let frontmatter = (title: \"D\", draft: true,)\nbody",
    );
    let cfg = site.config();
    let path = site.root.join("content/posts/d.typ");
    let page = Page::load("posts", &path, &cfg, &project(&cfg)).unwrap();
    assert!(page.skipped(false, false));
    assert!(!page.skipped(true, false));
}

#[test]
fn empty_content_dir_returns_empty() {
    let site = Site::new();
    site.write(
        "config.kdl",
        "site \"T\"\npaths {\n  content \"content\"\n}",
    );
    fs::create_dir_all(site.root.join("content")).unwrap();
    let cfg = site.config();
    let collections = discover(&cfg, &project(&cfg)).unwrap();
    assert!(collections.is_empty());
}

#[test]
fn missing_content_dir_returns_empty() {
    let site = Site::new();
    site.write(
        "config.kdl",
        "site \"T\"\npaths {\n  content \"content\"\n}",
    );
    let cfg = site.config();
    let collections = discover(&cfg, &project(&cfg)).unwrap();
    assert!(collections.is_empty());
}

#[test]
fn nested_dirs_traversed() {
    let site = Site::new();
    site.write("config.kdl", "site \"T\"\nclean #true");
    site.write(
        "content/posts/2024/jan.typ",
        "#let frontmatter = (title: \"Jan\",)\nbody",
    );
    site.write(
        "content/posts/2024/feb.typ",
        "#let frontmatter = (title: \"Feb\",)\nbody",
    );
    let cfg = site.config();
    let collections = discover(&cfg, &project(&cfg)).unwrap();
    let posts = collections.iter().find(|c| c.id == "posts").unwrap();
    assert_eq!(posts.pages.len(), 2);
}

#[test]
fn hidden_dirs_skipped() {
    let site = Site::new();
    site.write("config.kdl", "site \"T\"");
    site.write(
        "content/posts/real.typ",
        "#let frontmatter = (title: \"R\",)\nbody",
    );
    site.write(
        "content/.drafts/fake.typ",
        "#let frontmatter = (title: \"F\",)\nbody",
    );
    let cfg = site.config();
    let collections = discover(&cfg, &project(&cfg)).unwrap();
    assert_eq!(collections.len(), 1);
    assert_eq!(collections[0].pages.len(), 1);
}

#[test]
fn bundle_index_takes_slug_from_its_directory() {
    let site = Site::new();
    site.write(
        "config.kdl",
        r#"
            site "T"
            paths { content "content" dist "public" }
            collections { posts "posts/**/*.typ" permalink="/posts/{slug}/" }
        "#,
    );
    // A page bundle: the directory name is the slug, not the `index` filename.
    site.write(
        "content/posts/ring-buffers/index.typ",
        "#let frontmatter = (title: \"RB\",)\nbody",
    );
    // A flat file keeps its stem.
    site.write(
        "content/posts/flat.typ",
        "#let frontmatter = (title: \"Flat\",)\nbody",
    );
    let out = site.run(&["build"]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        site.root
            .join("public/posts/ring-buffers/index.html")
            .exists(),
        "bundle dir slug"
    );
    assert!(
        site.root.join("public/posts/flat/index.html").exists(),
        "flat file slug"
    );
    assert!(
        !site.root.join("public/posts/index/index.html").exists(),
        "index must not be the slug"
    );
}
