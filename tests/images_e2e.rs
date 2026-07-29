//! Image handling that turns on bytes rather than markup.
//!
//! Externalization, sizing and `srcset` are scenarios (`tests/scenarios/images.kdl`).
//! What is left here compares raw bytes, captures a generated fingerprint, spans
//! two builds, or reads a warning off the CLI's stderr.

mod common;

use common::Site;

/// A tiny PNG of the given size, its pixels varying with position so two
/// different sizes never share bytes (and so collide only when we mean them to).
fn png(w: u32, h: u32) -> Vec<u8> {
    let img = image::RgbImage::from_fn(w, h, |x, y| {
        image::Rgb([(x * 7 + y * 13) as u8, (x * 3) as u8, (y * 5) as u8])
    });
    let mut buf = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgb8(img)
        .write_to(&mut buf, image::ImageFormat::Png)
        .unwrap();
    buf.into_inner()
}

const EXTRACT: &str = "site \"T\"\npaths {\n  content \"content\"\n  dist \"public\"\n}\nassets {\n  images { extract #true }\n}\n";

#[test]
fn typst_image_externalizes_to_a_file() {
    let site = Site::with(EXTRACT);
    site.write_bytes("content/pic.png", &png(2, 2));
    site.write("content/index.typ", "#image(\"pic.png\")\n");

    site.stats();
    let html = site.output("index.html");
    assert!(
        html.contains("src=\"/assets/pic.png\""),
        "image references the externalized file, not a data URI: {html}"
    );
    assert!(
        !html.contains("data:image"),
        "no inline base64 remains: {html}"
    );
    assert!(site.exists("public/assets/pic.png"), "the file was copied");
    // The copied bytes are the source bytes, untouched.
    assert_eq!(
        std::fs::read(site.path("public/assets/pic.png")).unwrap(),
        png(2, 2)
    );
}

#[cfg(feature = "css")]
#[test]
fn fingerprint_hashes_the_externalized_name() {
    let site = Site::with(
        "site \"T\"\npaths {\n  content \"content\"\n  dist \"public\"\n}\nassets {\n  fingerprint #true\n  images { extract #true }\n}\n",
    );
    site.write_bytes("content/pic.png", &png(2, 2));
    site.write("content/index.typ", "#image(\"pic.png\")\n");

    site.stats();
    let html = site.output("index.html");
    // pic.<16 hex>.png
    let marker = "src=\"/assets/pic.";
    let start = html.find(marker).expect("hashed reference") + marker.len();
    let hex: String = html[start..].chars().take_while(|c| *c != '.').collect();
    assert_eq!(hex.len(), 16, "16-char content hash in {html}");
    assert!(hex.chars().all(|c| c.is_ascii_hexdigit()));
    assert!(site.exists(&format!("public/assets/pic.{hex}.png")));
}

#[test]
fn cached_rebuild_keeps_the_externalized_file() {
    // The asset directory is regenerated every build, so a cache hit (which does
    // not recompile the page) must still re-copy the image from its stored ref.
    let site = Site::with(EXTRACT);
    site.write_bytes("content/pic.png", &png(2, 2));
    site.write("content/index.typ", "#image(\"pic.png\")\n");

    site.stats();
    let second = site.stats();
    assert!(second.cached >= 1, "the page was served from cache");
    assert!(
        site.exists("public/assets/pic.png"),
        "the file is present after a cached rebuild"
    );
}

#[test]
fn colliding_image_names_warn_and_keep_one() {
    // Two different sources with the same base name map to one served name (no
    // fingerprint to disambiguate). The build warns and keeps the first.
    let site = Site::with(EXTRACT);
    site.write_bytes("content/one/pic.png", &png(2, 2));
    site.write_bytes("content/two/pic.png", &png(3, 3));
    site.write("content/a.typ", "#image(\"one/pic.png\")\n");
    site.write("content/b.typ", "#image(\"two/pic.png\")\n");

    // Spawn the binary so the warning surfaces on stderr.
    let stderr = site.build();
    assert!(
        stderr.contains("two images map to") && stderr.contains("pic.png"),
        "collision warning surfaced: {stderr}"
    );
    assert!(site.exists("public/assets/pic.png"), "one file kept");
}

/// Processed bytes are memoized across builds: the pipeline used to re-run
/// oxipng and the downscale over every image on every build, including a fully
/// cached one.
#[test]
#[cfg(feature = "images")]
fn an_unchanged_image_is_not_re_encoded() {
    let site = Site::with(
        "site \"T\"\npaths {\n  content \"content\"\n  dist \"public\"\n  assets \"assets\"\n}\nassets {\n  images {\n      optimize { png level=2 }\n      responsive { widths 20 }\n    }\n}\n",
    );
    site.write_bytes("assets/big.png", &png(60, 40));
    site.write(
        "content/index.typ",
        "#html.elem(\"img\", attrs: (src: \"/assets/big.png\"))\n",
    );
    site.stats();
    let first = std::fs::read(site.path("public/assets/big.png")).unwrap();
    let variant = std::fs::read(site.path("public/assets/big-20.png")).unwrap();

    // The memo survives the asset tree being regenerated wholesale.
    site.stats();
    assert_eq!(
        std::fs::read(site.path("public/assets/big.png")).unwrap(),
        first,
        "a memoized rebuild must produce the same bytes"
    );
    assert_eq!(
        std::fs::read(site.path("public/assets/big-20.png")).unwrap(),
        variant
    );

    // ...and a changed source is re-encoded rather than served from the memo.
    site.write_bytes("assets/big.png", &png(60, 41));
    site.stats();
    assert_ne!(
        std::fs::read(site.path("public/assets/big.png")).unwrap(),
        first,
        "an edited image must not come from the memo"
    );
}
