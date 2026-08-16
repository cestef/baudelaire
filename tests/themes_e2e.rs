//! The themes shipped in `themes/`, built as real sites.

mod common;

use std::fs;
use std::path::Path;

use common::Site;

/// Every theme in `themes/`.
const THEMES: &[&str] = &["albatros", "spleen", "phares", "paysage"];

/// The two that are blogs, and so share one content shape.
const BLOGS: &[&str] = &["albatros", "spleen"];

/// Copy a directory tree: `theme "themes/x"` resolves inside the project, since
/// a Typst import cannot leave the project root.
fn copy(from: &Path, to: &Path) {
    fs::create_dir_all(to).expect("mkdir");
    for entry in fs::read_dir(from).expect("read theme dir") {
        let entry = entry.expect("dir entry");
        let (source, target) = (entry.path(), to.join(entry.file_name()));
        if entry.file_type().expect("file type").is_dir() {
            copy(&source, &target);
        } else {
            fs::copy(&source, &target).expect("copy");
        }
    }
}

/// An empty site wearing `theme`, with `extra` appended to its config.
fn wearing(theme: &str, extra: &str) -> Site {
    let site = Site::with(&format!(
        "site \"T\"\nurl \"https://example.net\"\nauthor \"A\"\ntheme \"themes/{theme}\"\n\
         paths {{ content \"content\"; dist \"public\"; assets \"assets\"; templates \"templates\" }}\n\
         {extra}"
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
    site
}

/// A two-post blog, the shape `albatros` and `spleen` are built around.
fn blog(theme: &str) -> Site {
    let site = wearing(theme, "");
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

#[test]
fn every_theme_builds_a_site() {
    for theme in THEMES {
        let site = wearing(theme, "");
        site.stats();

        let home = site.output("index.html");
        assert!(home.contains("Home"), "{theme}: title: {home}");
        assert!(home.contains("Welcome."), "{theme}: body: {home}");
        assert!(
            home.contains("/assets/style.css"),
            "{theme}: theme stylesheet linked: {home}"
        );
        assert!(
            site.exists("public/assets/style.css"),
            "{theme}: stylesheet"
        );
    }
}

#[test]
fn a_blog_theme_builds_posts_an_index_and_a_term_page() {
    for theme in BLOGS {
        let site = blog(theme);
        site.stats();

        let post = site.output("posts/first/index.html");
        assert!(post.contains("First"), "{theme}: post title: {post}");
        assert!(post.contains("Body text."), "{theme}: post body: {post}");
        assert!(post.contains("/tags/rust/"), "{theme}: tag links: {post}");
        // The pager reads `page.nav`, so it names the sibling itself.
        assert!(post.contains("Second"), "{theme}: pager: {post}");

        let index = site.output("posts/index.html");
        assert!(index.contains("/posts/first/"), "{theme}: index: {index}");
        assert!(
            index.contains("A summary."),
            "{theme}: entry summary: {index}"
        );

        let term = site.output("tags/rust/index.html");
        assert!(term.contains("/posts/second/"), "{theme}: term: {term}");
    }
}

/// A `home.typ` page lists the collection from `@baudelaire/pages`, the build's
/// own catalogue.
#[test]
fn a_blog_theme_lists_recent_posts_on_a_home_page() {
    for theme in BLOGS {
        let site = blog(theme);
        site.write(
            "content/index.typ",
            "#let frontmatter = (title: \"Home\", template: \"home.typ\")\n\nWelcome.\n",
        );
        site.stats();

        let home = site.output("index.html");
        assert!(home.contains("Welcome."), "{theme}: own body: {home}");
        assert!(
            home.contains("/posts/first/") && home.contains("/posts/second/"),
            "{theme}: catalogue rows: {home}"
        );
    }
}

/// `spleen` is the one that ships no script at all.
#[test]
fn the_terminal_theme_emits_no_script() {
    let site = blog("spleen");
    site.stats();

    let post = site.output("posts/first/index.html");
    assert!(!post.contains("<script"), "no script: {post}");
}

/// `albatros` is the multilingual one: its switcher is built from the page's
/// own editions, and its labels come from the language's string table.
#[test]
fn the_blog_theme_switches_languages() {
    let site = blog("albatros");
    site.write(
        "config.kdl",
        "site \"T\"\nurl \"https://example.net\"\ntheme \"themes/albatros\"\n\
         paths { content \"content\"; dist \"public\"; assets \"assets\"; templates \"templates\" }\n\
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
    // typst's own `display` knows English month names only, so the localized
    // date comes from baudelaire.
    assert!(french.contains("juillet"), "localized date: {french}");
}

/// `phares` is the documentation one: a sidebar of the site's own tree, a
/// search client to open, and a contents placeholder its script fills in.
#[test]
fn the_docs_theme_builds_a_sidebar_and_a_search_client() {
    let site = wearing("phares", "");
    site.write(
        "content/guide/install.typ",
        "#let frontmatter = (title: \"Install\", order: 1)\n\nInstall it.\n",
    );
    site.write(
        "content/guide/writing.typ",
        "#let frontmatter = (title: \"Writing\", order: 2)\n\n= A heading\n\nWrite it.\n",
    );
    site.stats();

    let page = site.output("guide/install/index.html");
    assert!(
        page.contains("class=\"sidebar\"") && page.contains("/guide/writing/"),
        "sidebar names the site's own pages: {page}"
    );
    assert!(page.contains("data-toc"), "contents placeholder: {page}");
    assert!(
        page.contains("data-search-open") && page.contains("/search.js"),
        "search trigger and client: {page}"
    );
    assert!(page.contains("Writing"), "pager: {page}");
    assert!(site.exists("public/search.json"), "search index");
}

/// `paysage` is the portfolio one: the landing page's grid is the catalogue,
/// and a project page shows the facts its own frontmatter carries.
#[test]
fn the_portfolio_theme_builds_a_work_grid_and_a_case_study() {
    let site = wearing("paysage", "");
    site.write(
        "content/index.typ",
        "#let frontmatter = (title: \"Studio\", template: \"home.typ\", tagline: \"Work.\")\n\nHello.\n",
    );
    site.write(
        "content/work/ledger.typ",
        "#let frontmatter = (\n  title: \"Ledger\",\n  date: datetime(year: 2026, month: 3, day: 1),\n  summary: \"Books that balance.\",\n  role: \"Build\",\n  stack: (\"rust\",),\n)\n\nIt balances.\n",
    );
    site.stats();

    let home = site.output("index.html");
    assert!(home.contains("class=\"grid\""), "work grid: {home}");
    assert!(home.contains("/work/ledger/"), "catalogue row: {home}");
    assert!(home.contains("2026"), "the year, not the full date: {home}");

    let project = site.output("work/ledger/index.html");
    assert!(project.contains("Role"), "fact from frontmatter: {project}");
    assert!(project.contains("/stack/rust/"), "term link: {project}");

    let term = site.output("stack/rust/index.html");
    assert!(term.contains("/work/ledger/"), "term page: {term}");
}

#[test]
fn a_project_file_still_overrides_a_shipped_theme() {
    let site = blog("albatros");
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

#[test]
fn every_theme_navigates_to_the_sections_it_finds() {
    for theme in THEMES {
        // Each theme's own content shape, and the link it draws for it: a docs
        // sidebar lists the pages, everything else links the section index.
        let (site, link) = match *theme {
            "phares" => {
                let site = wearing(theme, "");
                site.write(
                    "content/guide/install.typ",
                    "#let frontmatter = (title: \"Install\")\n\nInstall it.\n",
                );
                (site, "/guide/install/")
            }
            "paysage" => {
                let site = wearing(theme, "");
                site.write(
                    "content/work/ledger.typ",
                    "#let frontmatter = (title: \"Ledger\", date: datetime(year: 2026, month: 3, day: 1))\n\nIt balances.\n",
                );
                (site, "/work/")
            }
            _ => (blog(theme), "/posts/"),
        };
        site.stats();

        let home = site.output("index.html");
        assert!(
            home.contains(&format!("href=\"{link}\"")),
            "{theme}: derived nav: {home}"
        );
    }
}
