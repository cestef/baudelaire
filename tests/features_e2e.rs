//! In-process full-site build exercising taxonomies, pagination, feeds, robots,
//! and the sitemap. Unlike the binary-spawning e2e tests, this drives the engine
//! inside the test process, so the generators are actually measured by coverage.

mod common;

use baudelaire::engine::{Engine, Mode};
use baudelaire::ui::{Level, Ui};

use common::Site;

fn silent() -> Ui {
    Ui::new(Level::Silent)
}

const CONFIG: &str = r#"
site "T"
url "https://example.com"
paths {
    content "content"
    dist "public"
}
collections {
    blog sort="date" reverse=#true paginate=2
}
taxonomies {
    tags index=#true
}
output {
    sitemap #true
    robots {
        disallow "/drafts/"
    }
    feed {
        formats "rss" "atom"
    }
}
"#;

/// A blog post with a date and tags in its frontmatter.
fn post(title: &str, day: u8, tags: &str) -> String {
    format!(
        "#let frontmatter = (title: \"{title}\", \
         date: datetime(year: 2024, month: 1, day: {day}), tags: ({tags}))\n{title} body\n"
    )
}

#[test]
fn full_site_generates_taxonomies_pagination_feeds_and_metadata() {
    let site = Site::with(CONFIG);
    site.write("content/blog/a.typ", &post("Alpha", 1, "\"rust\","));
    site.write("content/blog/b.typ", &post("Bravo", 2, "\"rust\", \"cli\""));
    site.write("content/blog/c.typ", &post("Charlie", 3, "\"cli\","));

    let stats = Engine::new(site.config(), Mode::Build)
        .expect("engine")
        .build(&silent())
        .expect("build");
    assert!(stats.pages >= 3, "at least the 3 posts built, got {}", stats.pages);

    // Pagination: paginate=2 over 3 posts -> page 1 plus a page 2.
    assert!(site.exists("public/blog/index.html"), "paginated index");
    assert!(site.exists("public/blog/page/2/index.html"), "second page");

    // Taxonomy: an index plus one page per term.
    assert!(site.exists("public/tags/index.html"), "tag index");
    assert!(site.exists("public/tags/rust/index.html"), "rust term");
    assert!(site.exists("public/tags/cli/index.html"), "cli term");

    // Sitemap + robots (base url is set, so robots links the sitemap).
    assert!(site.exists("public/sitemap.xml"), "sitemap");
    let robots = site.read("public/robots.txt");
    assert!(robots.contains("Disallow: /drafts/"), "robots: {robots}");
    assert!(
        robots.contains("Sitemap: https://example.com/sitemap.xml"),
        "robots sitemap link: {robots}"
    );

    // A feed was emitted (rss + atom enabled).
    let files = site.files("public");
    assert!(
        files.iter().any(|f| f.contains("rss") || f.contains("atom") || f.contains("feed")),
        "a feed file exists: {files:?}"
    );
}

const FLAT_CONFIG: &str = r#"
site "T"
url "https://example.com"
paths {
    content "content"
    dist "public"
}
output {
    urls "flat"
    search {
        formats "json"
        fields "title" "body"
    }
    llms {
        summary "A test site."
    }
}
"#;

#[test]
fn flat_urls_with_redirects_search_and_llms() {
    let site = Site::with(FLAT_CONFIG);
    site.write(
        "content/about.typ",
        "#let frontmatter = (title: \"About\", redirect: (\"/old-about/\",))\nAbout body\n",
    );
    site.write(
        "content/index.typ",
        "#let frontmatter = (title: \"Home\",)\nWelcome\n",
    );

    let stats = Engine::new(site.config(), Mode::Build)
        .expect("engine")
        .build(&silent())
        .expect("build");
    assert!(stats.pages >= 2, "home + about built, got {}", stats.pages);

    // Flat URLs write `.html` files rather than `dir/index.html`.
    assert!(site.exists("public/about.html"), "flat about page");

    // A redirect stub forwards the stale path to the page's permalink.
    let stub = site.read("public/old-about.html");
    assert!(stub.contains("http-equiv"), "meta-refresh redirect: {stub}");
    assert!(stub.to_lowercase().contains("redirecting"), "redirect body: {stub}");

    // Client-side search index (formats "json").
    assert!(site.exists("public/search.json"), "search index");
}

const ASSET_CONFIG: &str = r#"
site "T"
paths {
    content "content"
    dist "public"
    assets "assets"
}
output {
    assets {
        minify #true
        bundle #true
        fingerprint #true
    }
}
"#;

#[test]
fn asset_pipeline_processes_css_js_and_images() {
    let site = Site::with(ASSET_CONFIG);
    site.write("content/index.typ", "#let frontmatter = (title: \"H\",)\nhi\n");
    // CSS with a url() reference so the fingerprint-rewrite path runs.
    site.write("assets/style.css", "body { color: #ff0000; background: url(\"pic.png\"); }\n");
    site.write("assets/app.js", "export const answer = 41 + 1;\n");
    let png = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/bloated.png")).unwrap();
    site.write_bytes("assets/pic.png", &png);

    let stats = Engine::new(site.config(), Mode::Build)
        .expect("engine")
        .build(&silent())
        .expect("build");
    assert!(stats.pages >= 1);

    // Each asset kind lands under dist/assets, fingerprinted.
    let out = site.files("public/assets");
    assert!(out.iter().any(|f| f.ends_with(".css")), "css emitted: {out:?}");
    assert!(out.iter().any(|f| f.ends_with(".js")), "js emitted: {out:?}");
    assert!(out.iter().any(|f| f.ends_with(".png")), "png emitted: {out:?}");
}
