//! The themes shipped in `themes/`, built as real sites.
//!
//! A theme is Typst that nothing else in the tree compiles, so without this it
//! rots silently: a template still parses, and the first person to name the
//! theme finds out. Each case copies one theme into a throwaway site and builds
//! posts, an index, and a term page through it.

mod common;

use std::fs;
use std::path::Path;

use common::Site;

/// Every theme in `themes/`, named so a missing case is a compile error rather
/// than a test that quietly never runs.
const THEMES: &[&str] = &["albatros", "spleen", "voyage"];

/// Copy a directory tree, which is how a theme gets from the repository into a
/// site: `theme "themes/x"` resolves inside the project, since a Typst import
/// cannot leave the project root.
fn copy(from: &Path, to: &Path) {
    fs::create_dir_all(to).expect("mkdir");
    for entry in fs::read_dir(from).expect("read theme dir") {
        let entry = entry.expect("dir entry");
        let (source, target) = (entry.path(), to.join(entry.file_name()));
        match entry.file_type().expect("file type").is_dir() {
            true => copy(&source, &target),
            false => {
                fs::copy(&source, &target).expect("copy");
            }
        }
    }
}

/// A two-post site wearing `theme`, with everything the shipped `theme.kdl`
/// generates: a paginated index, a tag taxonomy, feeds.
fn site(theme: &str) -> Site {
    let site = Site::with(&format!(
        "site \"T\"\nurl \"https://example.net\"\nauthor \"A\"\ntheme \"themes/{theme}\"\n\
         paths {{ content \"content\" dist \"public\" assets \"assets\" templates \"templates\" }}\n"
    ));
    copy(
        &Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("themes")
            .join(theme),
        &site.path(&format!("themes/{theme}")),
    );
    site.write(
        "content/index.typ",
        "#let frontmatter = (title: \"Home\", template: \"page.typ\")\n\nWelcome.\n",
    );
    site.write(
        "content/posts/first.typ",
        "#let frontmatter = (\n  title: \"First\",\n  date: datetime(year: 2026, month: 7, day: 20),\n  tags: (\"rust\",),\n  summary: \"A summary.\",\n)\n\n= Heading\n\nBody text.\n",
    );
    site.write(
        "content/posts/second.typ",
        "#let frontmatter = (\n  title: \"Second\",\n  date: datetime(year: 2026, month: 7, day: 28),\n  tags: (\"rust\",),\n)\n\nMore body text.\n",
    );
    site
}

/// Each theme renders a post, its own index, and a term page, with the layout,
/// the stylesheet, and the entry data all wired up.
#[test]
fn every_theme_builds_a_site() {
    for theme in THEMES {
        let site = site(theme);
        site.stats();

        let post = site.output("posts/first/index.html");
        assert!(post.contains("First"), "{theme}: post title: {post}");
        assert!(post.contains("Body text."), "{theme}: post body: {post}");
        assert!(
            post.contains("/assets/style.css"),
            "{theme}: theme stylesheet linked: {post}"
        );
        assert!(post.contains("/tags/rust/"), "{theme}: tag links: {post}");
        // The pager reads `page.nav`, so it names the sibling rather than a URL
        // the theme guessed at.
        assert!(post.contains("Second"), "{theme}: pager: {post}");

        let index = site.output("posts/index.html");
        assert!(index.contains("/posts/first/"), "{theme}: index: {index}");
        assert!(
            index.contains("A summary."),
            "{theme}: entry summary: {index}"
        );

        let term = site.output("tags/rust/index.html");
        assert!(term.contains("/posts/second/"), "{theme}: term: {term}");

        // The theme's assets are published beside a project's own.
        assert!(
            site.exists("public/assets/style.css"),
            "{theme}: stylesheet"
        );
    }
}

/// The nav is derived from the build's own view of `content/`, so it names the
/// site's real directories instead of a menu the theme hardcoded.
#[test]
fn every_theme_navigates_to_the_sections_it_finds() {
    for theme in THEMES {
        let site = site(theme);
        site.stats();

        let home = site.output("index.html");
        assert!(
            home.contains("href=\"/posts/\""),
            "{theme}: derived nav: {home}"
        );
    }
}

/// A project file at the same relative path wins, which is what makes a theme
/// adjustable without forking it.
#[test]
fn a_project_file_still_overrides_a_shipped_theme() {
    let site = site("albatros");
    site.write(
        "templates/page.typ",
        "#let page(page, body) = html.elem(\"article\", body)\n",
    );
    site.stats();

    let post = site.output("posts/first/index.html");
    assert!(post.contains("<article>"), "project template wins: {post}");
    assert!(
        !post.contains("class=\"site-header\""),
        "theme shell gone: {post}"
    );
}

/// `voyage` is the multilingual one: its switcher is built from the page's own
/// editions, and its labels come from the language's string table.
#[test]
fn the_multilingual_theme_switches_languages() {
    let site = site("voyage");
    site.write(
        "config.kdl",
        "site \"T\"\nurl \"https://example.net\"\ntheme \"themes/voyage\"\n\
         paths { content \"content\" dist \"public\" assets \"assets\" templates \"templates\" }\n\
         lang \"en\"\nlanguages {\n  en { name \"English\" }\n  fr { name \"Français\"\n    strings {\n      reading \"min de lecture\"\n      date \"{day} {month} {year}\"\n      months \"janvier\" \"février\" \"mars\" \"avril\" \"mai\" \"juin\" \"juillet\" \"août\" \"septembre\" \"octobre\" \"novembre\" \"décembre\"\n    }\n  }\n}\n",
    );
    site.write(
        "content/posts/first.fr.typ",
        "#let frontmatter = (\n  title: \"Premier\",\n  date: datetime(year: 2026, month: 7, day: 20),\n)\n\nDu texte.\n",
    );
    site.stats();

    let english = site.output("posts/first/index.html");
    assert!(
        english.contains("hreflang=\"fr\"") && english.contains("Français"),
        "switcher names the other edition: {english}"
    );

    let french = site.output("fr/posts/first/index.html");
    assert!(
        french.contains("min de lecture"),
        "labels come from the string table: {french}"
    );
    // The date is written the way French writes it, by baudelaire rather than by
    // the theme: typst's own `display` knows English month names only.
    assert!(french.contains("juillet"), "localized date: {french}");
}
