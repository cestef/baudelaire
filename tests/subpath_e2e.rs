//! Subpath hosting: a site whose `url` carries a path is served under that
//! directory. On-page URLs shift under the prefix; the on-disk layout and the
//! canonical absolute URLs stay correct.

mod common;

use common::{Serve, Site, wait_for_port};

/// A site served from `/docs`, with an internal link, a raw absolute link, a
/// redirect, and the search client.
fn subsite() -> Site {
    let site = Site::with(
        r#"
        site "T"
        url "https://host.test/docs"
        paths {
            content "content"
            dist "public"
        }
        output {
            sitemap #true
            search { formats "json"; client #true }
        }
        serve { open #false }
        "#,
    );
    site.write(
        "content/posts/hello.typ",
        "#let frontmatter = (title: \"Hello\", redirect: (\"/old/\",))\n\
         See #link(\"world.typ\")[world] and #link(\"/manual/\")[manual].\n",
    );
    site.write(
        "content/posts/world.typ",
        "#let frontmatter = (title: \"World\",)\nWorld.\n",
    );
    site
}

#[test]
fn on_page_urls_shift_under_the_base_path() {
    let site = subsite();
    site.build();
    let html = site.output("posts/hello/index.html");

    // Internal `.typ` link and a raw absolute link both gain the prefix.
    assert!(
        html.contains("href=\"/docs/posts/world/\""),
        "internal link not prefixed: {html}"
    );
    assert!(
        html.contains("href=\"/docs/manual/\""),
        "raw absolute link not prefixed: {html}"
    );
}

#[test]
fn disk_layout_is_unprefixed() {
    let site = subsite();
    site.build();
    // Files land at their canonical path, never under a `docs/` directory.
    assert!(site.exists("public/posts/hello/index.html"));
    assert!(!site.exists("public/docs"));
}

#[test]
fn absolute_urls_carry_the_subpath() {
    let site = subsite();
    site.build();
    let html = site.output("posts/hello/index.html");
    assert!(
        html.contains("href=\"https://host.test/docs/posts/hello/\""),
        "canonical missing subpath: {html}"
    );
    let sitemap = site.output("sitemap.xml");
    assert!(
        sitemap.contains("https://host.test/docs/posts/hello/"),
        "sitemap missing subpath: {sitemap}"
    );
}

#[test]
fn redirect_target_is_prefixed() {
    let site = subsite();
    site.build();
    let stub = site.output("old/index.html");
    assert!(
        stub.contains("/docs/posts/hello/"),
        "redirect target not prefixed: {stub}"
    );
}

#[test]
fn search_client_carries_the_base() {
    let site = subsite();
    site.build();
    let js = site.output("search.js");
    assert!(
        js.contains("const BASE = \"/docs\""),
        "no BASE in client: {js}"
    );
    // The index data stays canonical; the client applies the base.
    let json = site.output("search.json");
    assert!(
        json.contains("\"/posts/hello/\""),
        "index url not canonical: {json}"
    );
}

#[test]
fn root_hosting_leaves_urls_untouched() {
    let site = Site::with(
        "site \"T\"\nurl \"https://host.test\"\npaths {\n  content \"content\"\n  dist \"public\"\n}\n",
    );
    site.write(
        "content/posts/hello.typ",
        "#let frontmatter = (title: \"H\",)\n#link(\"/manual/\")[m]\n",
    );
    site.build();
    let html = site.output("posts/hello/index.html");
    assert!(
        html.contains("href=\"/manual/\""),
        "root link mangled: {html}"
    );
}

#[test]
fn serve_previews_under_the_base_path() {
    let site = subsite();
    site.build();
    let server = Serve::start(&site, &["--no-watch"]);
    assert!(wait_for_port(server.port(), 5000));
    // The prefixed request resolves to the unprefixed file on disk.
    assert_eq!(server.get("/docs/posts/hello/").0, 200);
    assert_eq!(server.get("/docs/posts/world/").0, 200);
}
