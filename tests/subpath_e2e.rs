//! Subpath hosting: a site whose `url` carries a path is served under that
//! directory.

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
        serve { open #false }
        generate {
          sitemap #true
          search { ui }
        }
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
    site.stats();
    let html = site.output("posts/hello/index.html");

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
    site.stats();
    assert!(site.exists("public/posts/hello/index.html"));
    assert!(!site.exists("public/docs"));
}

#[test]
fn absolute_urls_carry_the_subpath() {
    let site = subsite();
    site.stats();
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
    site.stats();
    let stub = site.output("old/index.html");
    assert!(
        stub.contains("/docs/posts/hello/"),
        "redirect target not prefixed: {stub}"
    );
}

#[test]
fn search_client_carries_the_base() {
    let site = subsite();
    site.stats();
    let js = site.output("search.js");
    assert!(
        js.contains("\"/docs/search.json\""),
        "no prefixed index in client: {js}"
    );
    let json = site.output("search.json");
    assert!(
        json.contains("\"base\":\"/docs\""),
        "index does not carry the base: {json}"
    );
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
    site.stats();
    let html = site.output("posts/hello/index.html");
    assert!(
        html.contains("href=\"/manual/\""),
        "root link mangled: {html}"
    );
}

#[test]
fn serve_previews_under_the_base_path() {
    let site = subsite();
    site.stats();
    let server = Serve::start(&site, &["--no-watch"]);
    assert!(wait_for_port(server.port(), 5000));
    assert_eq!(server.get("/docs/posts/hello/").0, 200);
    assert_eq!(server.get("/docs/posts/world/").0, 200);
}

/// `BasePath` only walks the DOM, so a root-absolute URL rewritten inside CSS
/// has to be prefixed separately.
#[test]
#[cfg(feature = "css")]
fn css_references_carry_the_subpath() {
    let site = Site::with(
        r#"
        site "T"
        url "https://host.test/docs"
        paths {
          content "content"
          dist "public"
          assets "assets"
        }
        assets {
          fingerprint #true
        }
        "#,
    );
    site.write("assets/logo.svg", "<svg/>\n");
    site.write("assets/app.css", "body { background: url(logo.svg) }\n");
    site.write(
        "content/index.typ",
        "#let frontmatter = (title: \"H\",)\nhome\n",
    );
    site.stats();

    let sheet = site
        .files("public/assets")
        .into_iter()
        .find(|name| name.starts_with("app.") && common::has_ext(name, "css"))
        .expect("fingerprinted stylesheet");
    let css = site.read(&format!("public/assets/{sheet}"));
    assert!(css.contains("/docs/assets/logo."), "{css}");
}

/// `og:image` carries its URL in a `content` attribute rather than an `href`.
#[test]
fn og_image_carries_the_subpath() {
    let site = Site::with(
        r#"
        site "T"
        url "https://host.test/docs"
        paths {
            content "content"
            dist "public"
            assets "assets"
        }
        "#,
    );
    site.write("assets/card.png", "not really a png");
    site.write(
        "content/index.typ",
        "#let frontmatter = (title: \"H\", image: \"/assets/card.png\",)\nhome\n",
    );
    site.stats();

    let html = site.output("index.html");
    assert!(html.contains("og:image"), "{html}");
    assert!(html.contains("/docs/assets/card.png"), "{html}");
}

/// ...and nothing else in a `content` attribute is a URL: a title or a
/// description that happens to start with `/` is prose.
#[test]
fn a_base_path_leaves_a_meta_tag_s_prose_alone() {
    let site = Site::with(
        r#"
        site "T"
        url "https://host.test/docs"
        description "/usr/share, explained"
        paths { content "content"; dist "public" }
        "#,
    );
    site.write(
        "content/index.typ",
        "#let frontmatter = (title: \"/etc/hosts, annotated\",)\nhome\n",
    );
    site.stats();

    let html = site.output("index.html");
    assert!(
        html.contains(r#"content="/etc/hosts, annotated""#),
        "a title is not a URL: {html}"
    );
    assert!(
        !html.contains("/docs/etc/hosts"),
        "the base path was applied to prose: {html}"
    );
}
