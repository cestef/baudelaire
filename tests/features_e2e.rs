//! In-process full-site build exercising taxonomies, pagination, feeds, robots,
//! and the sitemap.

mod common;

use baudelaire::engine::{Engine, Mode};

use common::{Site, has_ext, silent};

const CONFIG: &str = r#"
site "T"
url "https://example.com"
paths {
  content "content"
  dist "public"
}
content {
  collections {
      blog { sort "date"; reverse #true; paginate { size 2 } }
  }
  taxonomies {
      tags listing=#true
  }
}
generate {
  sitemap #true
  robots {
          disallow "/drafts/"
      }
  feed {
          formats "rss" "atom"
      }
}
"#;

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
    assert!(
        stats.pages >= 3,
        "at least the 3 posts built, got {}",
        stats.pages
    );

    assert!(site.exists("public/blog/index.html"), "paginated index");
    assert!(site.exists("public/blog/page/2/index.html"), "second page");

    assert!(site.exists("public/tags/index.html"), "tag index");
    assert!(site.exists("public/tags/rust/index.html"), "rust term");
    assert!(site.exists("public/tags/cli/index.html"), "cli term");

    assert!(site.exists("public/sitemap.xml"), "sitemap");
    let robots = site.read("public/robots.txt");
    assert!(robots.contains("Disallow: /drafts/"), "robots: {robots}");
    assert!(
        robots.contains("Sitemap: https://example.com/sitemap.xml"),
        "robots sitemap link: {robots}"
    );

    let files = site.files("public");
    assert!(
        files
            .iter()
            .any(|f| f.contains("rss") || f.contains("atom") || f.contains("feed")),
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
links {
  style "flat"
}
generate {
  sitemap #true
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

    assert!(site.exists("public/about.html"), "flat about page");

    let about = site.read("public/about.html");
    assert!(
        about.contains("/about.html") && !about.contains("\"/about/\""),
        "canonical does not name the file written: {about}"
    );
    let sitemap = site.read("public/sitemap.xml");
    assert!(sitemap.contains("/about.html"), "{sitemap}");
    assert!(!sitemap.contains("/about/<"), "{sitemap}");
    let search = site.read("public/search.json");
    assert!(search.contains("/about.html"), "{search}");

    let stub = site.read("public/old-about.html");
    assert!(stub.contains("http-equiv"), "meta-refresh redirect: {stub}");
    assert!(stub.contains("/about.html"), "redirect target: {stub}");
    assert!(
        stub.to_lowercase().contains("redirecting"),
        "redirect body: {stub}"
    );

    assert!(site.exists("public/search.json"), "search index");
}

const ASSET_CONFIG: &str = r#"
site "T"
paths {
  content "content"
  dist "public"
  assets "assets"
}
assets {
  minify #true
          bundle #true
          fingerprint #true
}
"#;

#[test]
fn asset_pipeline_processes_css_js_and_images() {
    let site = Site::with(ASSET_CONFIG);
    site.write(
        "content/index.typ",
        "#let frontmatter = (title: \"H\",)\nhi\n",
    );
    // The `url()` reference is what makes the fingerprint-rewrite path run.
    site.write(
        "assets/style.css",
        "body { color: #ff0000; background: url(\"pic.png\"); }\n",
    );
    site.write("assets/app.js", "export const answer = 41 + 1;\n");
    let png = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/bloated.png"
    ))
    .unwrap();
    site.write_bytes("assets/pic.png", &png);

    let stats = Engine::new(site.config(), Mode::Build)
        .expect("engine")
        .build(&silent())
        .expect("build");
    assert!(stats.pages >= 1);

    let out = site.files("public/assets");
    assert!(
        out.iter().any(|f| has_ext(f, "css")),
        "css emitted: {out:?}"
    );
    assert!(out.iter().any(|f| has_ext(f, "js")), "js emitted: {out:?}");
    assert!(
        out.iter().any(|f| has_ext(f, "png")),
        "png emitted: {out:?}"
    );
}
