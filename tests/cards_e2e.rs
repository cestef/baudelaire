//! Generated social cards.

#![cfg(feature = "cards")]

mod common;

use common::Site;

/// The PNG header's declared dimensions, so a test asserts on the real image
/// rather than on the file merely existing.
fn dimensions(bytes: &[u8]) -> (u32, u32) {
    let at = |i: usize| u32::from_be_bytes(bytes[i..i + 4].try_into().expect("4 bytes"));
    assert_eq!(&bytes[1..4], b"PNG", "not a PNG");
    (at(16), at(20))
}

/// A site with a card template that paints the whole page, so the card is
/// unmistakably the template's output and not a blank default.
fn site(config: &str) -> Site {
    let site = Site::with(&format!(
        r#"
        site "T"
        url "https://example.com"
        paths {{ content "content"; dist "public"; templates "templates" }}
        generate {{ {config} }}
        "#
    ));
    site.write(
        "templates/card.typ",
        "#let card(data) = rect(width: 100%, height: 100%, fill: rgb(\"#123456\"))[\n\
         #text(size: 48pt)[#data.title]\n]\n",
    );
    site
}

#[test]
fn a_card_is_rendered_at_the_configured_size_and_named_as_og_image() {
    let site = site(r#"cards { template "card.typ"; width 800; height 418 }"#);
    site.write(
        "content/posts/hello.typ",
        "#let frontmatter = (title: \"Hello\",)\nbody",
    );
    site.stats();

    let png = std::fs::read(site.path("public/cards/posts/hello.png")).expect("card");
    assert_eq!(dimensions(&png), (800, 418));

    let html = site.output("posts/hello/index.html");
    assert!(
        html.contains(r#"content="https://example.com/cards/posts/hello.png""#),
        "{html}"
    );
    assert!(html.contains("summary_large_image"), "{html}");
}

/// An authored image is the author's decision; a generated card never overrides
/// it, and none is rendered.
#[test]
fn an_authored_image_wins_and_skips_the_render() {
    let site = site(r#"cards { template "card.typ" }"#);
    site.write(
        "content/posts/shot.typ",
        "#let frontmatter = (title: \"Shot\", image: \"/screenshot.png\",)\nbody",
    );
    site.stats();

    assert!(!site.exists("public/cards/posts/shot.png"));
    let html = site.output("posts/shot/index.html");
    assert!(html.contains("/screenshot.png"), "{html}");
}

/// Generated listings get no card: nobody shares a tag index, and one render per
/// term would dominate the build.
#[test]
fn generated_listings_get_no_card() {
    let site = site(r#"cards { template "card.typ" }"#);
    site.write("config.kdl", &{
        let mut config = site.read("config.kdl");
        config.push_str("content {\n  taxonomies {\n    tags index=#true\n  }\n}\n");
        config
    });
    site.write(
        "content/posts/a.typ",
        "#let frontmatter = (title: \"A\", tags: (\"rust\",),)\nbody",
    );
    site.stats();

    assert!(site.exists("public/cards/posts/a.png"));
    assert!(
        site.exists("public/tags/rust/index.html"),
        "term page built"
    );
    assert!(!site.exists("public/cards/tags/rust.png"));
}

/// Off unless asked for: rendering a page per card is the most expensive thing
/// a build can do per page.
#[test]
fn no_cards_without_the_block() {
    let site = site("");
    site.write(
        "content/posts/hello.typ",
        "#let frontmatter = (title: \"Hello\",)\nbody",
    );
    site.stats();

    assert!(!site.exists("public/cards/posts/hello.png"));
    let html = site.output("posts/hello/index.html");
    assert!(!html.contains("og:image"), "{html}");
}
