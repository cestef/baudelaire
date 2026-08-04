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
        config.push_str("content {\n  taxonomies {\n    tags listing=#true\n  }\n}\n");
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

/// A card template is ordinary Typst and may import its own modules. The card
/// is a second compile of the page, so what that compile read is part of the
/// page's dependency set: editing a helper the template imports has to redraw
/// the card. Tracking only the template file left every card-bearing page a
/// cache hit, still serving the PNG the old helper drew.
#[test]
fn editing_a_module_the_card_template_imports_redraws_the_card() {
    let site = site(r#"cards { template "card.typ"; width 800; height 418 }"#);
    site.write("templates/palette.typ", "#let ink = rgb(\"#123456\")\n");
    site.write(
        "templates/card.typ",
        "#import \"palette.typ\": ink\n\
         #let card(data) = rect(width: 100%, height: 100%, fill: ink)\n",
    );
    site.write(
        "content/posts/hello.typ",
        "#let frontmatter = (title: \"Hello\",)\nbody",
    );
    site.stats();
    let before = std::fs::read(site.path("public/cards/posts/hello.png")).expect("card");

    // Only the imported module changes; the template itself is untouched.
    site.write("templates/palette.typ", "#let ink = rgb(\"#abcdef\")\n");
    let stats = site.stats();

    assert_eq!(
        (stats.pages, stats.cached),
        (1, 0),
        "the page draws a card from the edited module, so it cannot be reused"
    );
    let after = std::fs::read(site.path("public/cards/posts/hello.png")).expect("card");
    assert_ne!(before, after, "the card should have been redrawn");
}

/// A cache hit draws nothing, so the card in `dist` is still the one an earlier
/// build wrote. The sweep's keep set is derived from the pages and not from what
/// this build drew, or the first rebuild would delete every card on the site.
#[test]
fn a_cached_rebuild_keeps_the_card_it_did_not_redraw() {
    let site = site(r#"cards { template "card.typ"; width 800; height 418 }"#);
    site.write(
        "content/posts/hello.typ",
        "#let frontmatter = (title: \"Hello\",)\nbody",
    );
    site.stats();
    let before = std::fs::read(site.path("public/cards/posts/hello.png")).expect("card");

    let stats = site.stats();

    assert_eq!(
        (stats.pages, stats.cached),
        (1, 1),
        "nothing changed, so the page is reused and no card is drawn"
    );
    let after =
        std::fs::read(site.path("public/cards/posts/hello.png")).expect("the card survives");
    assert_eq!(before, after);
}

/// The counterpart: a card that is *gone* has to come back. The page's HTML is
/// rewritten from the cache on every build, but only a compile draws a card, so
/// a deleted `dist` with a warm cache left the file missing on every build from
/// then on. The page is stale when its card is absent.
#[test]
fn a_deleted_card_is_redrawn_even_though_the_page_is_otherwise_a_hit() {
    let site = site(r#"cards { template "card.typ"; width 800; height 418 }"#);
    site.write(
        "content/posts/hello.typ",
        "#let frontmatter = (title: \"Hello\",)\nbody",
    );
    site.stats();
    let before = std::fs::read(site.path("public/cards/posts/hello.png")).expect("card");

    std::fs::remove_dir_all(site.path("public")).expect("dist removed");
    let stats = site.stats();

    assert_eq!(
        (stats.pages, stats.cached),
        (1, 0),
        "the page's card is gone, so nothing about it can be reused"
    );
    let after = std::fs::read(site.path("public/cards/posts/hello.png")).expect("card redrawn");
    assert_eq!(before, after);
}

/// A page repaired for its backlinks keeps its card's template as a dependency.
///
/// A repair compiles the page's markup again and draws no sidecar, so the deps
/// it records name only the HTML compile's files. Recorded as-is they replaced
/// the entry the full compile wrote, and the card template stopped being an
/// input to the page: editing it changed no hash the cache checks, the page
/// stayed a hit, and `dist` kept serving the image the old template drew.
#[test]
fn a_repaired_page_keeps_the_card_template_as_a_dependency() {
    let site = site(r#"cards { template "card.typ"; width 800; height 418 }"#);
    site.write(
        "config.kdl",
        &format!(
            "{}
links {{ backlinks #true }}
",
            site.read("config.kdl").trim_end()
        ),
    );
    // b is linked from a, so a cold build compiles it against an empty
    // prediction and the repair pass compiles it a second time.
    site.write(
        "content/posts/a.typ",
        "#let frontmatter = (title: \"A\",)\n#link(\"b.typ\")[to b]",
    );
    site.write(
        "content/posts/b.typ",
        "#let frontmatter = (title: \"B\",)\nbeta",
    );
    site.stats();
    let before = std::fs::read(site.path("public/cards/posts/b.png")).expect("card");

    site.write(
        "templates/card.typ",
        "#let card(data) = rect(width: 100%, height: 100%, fill: rgb(\"#654321\"))[\n\
         #text(size: 48pt)[#data.title]\n]\n",
    );
    let stats = site.stats();

    assert_eq!(
        stats.cached, 0,
        "the card template every page draws through changed, so none may be reused"
    );
    let after = std::fs::read(site.path("public/cards/posts/b.png")).expect("card");
    assert_ne!(
        before, after,
        "the repaired page's card should have been redrawn"
    );
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

/// A card template a *theme* ships is the one the card draws with. `verify`
/// accepts a template supplied by either layer, so a project with no
/// `templates/card.typ` of its own passed the check and then failed every
/// card-bearing page on typst's own `file not found`: the card built its import
/// path out of the project's template directory alone.
#[test]
fn a_theme_supplies_the_card_template() {
    let site = Site::with(
        r#"
        site "T"
        url "https://example.com"
        theme "themes/plume"
        paths { content "content"; dist "public"; templates "templates" }
        generate { cards { template "card.typ"; width 800; height 418 } }
        "#,
    );
    site.write("themes/plume/theme.kdl", "");
    site.write(
        "themes/plume/templates/card.typ",
        "#let card(data) = rect(width: 100%, height: 100%, fill: rgb(\"#654321\"))[\n\
         #text(size: 48pt)[#data.title]\n]\n",
    );
    site.write(
        "content/posts/hello.typ",
        "#let frontmatter = (title: \"Hello\",)\nbody",
    );
    site.stats();

    let png = std::fs::read(site.path("public/cards/posts/hello.png")).expect("card");
    assert_eq!(dimensions(&png), (800, 418));
}
