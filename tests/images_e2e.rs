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
        // Masked rather than checked: the pattern is meant to wrap, so a
        // truncating cast is the point.
        image::Rgb([
            ((x * 7 + y * 13) & 0xff) as u8,
            ((x * 3) & 0xff) as u8,
            ((y * 5) & 0xff) as u8,
        ])
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

/// A page's `srcset` depends on the variants of the images it actually shows.
/// Folding the whole variant manifest into the build fingerprint (as this once
/// did) meant a change to any one image rebuilt every page on the site.
///
/// The image is resized rather than merely re-encoded, because that is what
/// moves the *manifest*: the recorded URLs are authored paths, so new pixels at
/// the same size leave it identical and would prove nothing.
#[test]
#[cfg(feature = "images")]
fn losing_one_images_variants_leaves_pages_that_do_not_show_it_cached() {
    let site = Site::with(
        "site \"T\"\npaths {\n  content \"content\"\n  dist \"public\"\n  assets \"assets\"\n}\nassets {\n  images {\n      responsive { widths 20 }\n    }\n}\n",
    );
    site.write_bytes("assets/shown.png", &png(60, 40));
    site.write(
        "content/shows.typ",
        "#let frontmatter = (title: \"Shows\",)\n#html.elem(\"img\", attrs: (src: \"/assets/shown.png\"))\n",
    );
    site.write(
        "content/plain.typ",
        "#let frontmatter = (title: \"Plain\",)\nno images here\n",
    );
    site.stats();
    assert!(site.output("shows/index.html").contains("srcset"));

    // Below the configured width, so the source drops out of the manifest.
    site.write_bytes("assets/shown.png", &png(10, 8));
    let stats = site.stats();

    assert_eq!(
        (stats.pages, stats.cached),
        (2, 1),
        "only the page showing the image depends on its variants"
    );
    assert!(
        !site.output("shows/index.html").contains("srcset"),
        "the page must lose the srcset it can no longer honour"
    );
}

/// The negative half. An image with no variants records that it had none, so
/// gaining some later still invalidates the page showing it. Recording only the
/// sources that matched would leave that page cached with no `srcset` at all.
#[test]
#[cfg(feature = "images")]
fn an_image_that_gains_variants_invalidates_the_page_showing_it() {
    let site = Site::with(
        "site \"T\"\npaths {\n  content \"content\"\n  dist \"public\"\n  assets \"assets\"\n}\nassets {\n  images {\n      responsive { widths 20 }\n    }\n}\n",
    );
    site.write_bytes("assets/pic.png", &png(10, 8));
    site.write(
        "content/index.typ",
        "#html.elem(\"img\", attrs: (src: \"/assets/pic.png\"))\n",
    );
    site.stats();
    assert!(
        !site.output("index.html").contains("srcset"),
        "a source narrower than the configured width has no variants"
    );

    // Now wide enough to downscale, so the manifest gains an entry.
    site.write_bytes("assets/pic.png", &png(60, 40));
    site.stats();

    let html = site.output("index.html");
    assert!(
        html.contains("srcset"),
        "the new variant must reach the page: {html}"
    );
}

/// A page showing an image that lives in the asset tree points at the pipeline's
/// copy: the file the pipeline optimized and named, written once. Extraction
/// used to stage the source bytes under that same name, which warned that two
/// images claimed it and shipped whichever won.
#[test]
fn an_image_from_the_asset_tree_is_not_extracted_a_second_time() {
    let site = Site::with(
        "site \"T\"\npaths {\n  content \"content\"\n  dist \"public\"\n  assets \"assets\"\n}\nassets {\n  images { extract #true }\n}\n",
    );
    site.write_bytes("assets/photo.png", &png(4, 4));
    site.write("content/index.typ", "#image(\"/assets/photo.png\")\n");

    let out = site.run(&["build"]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "stderr: {stderr}");
    assert!(
        !stderr.contains("images::collision"),
        "warned about its own asset: {stderr}"
    );
    let html = site.output("index.html");
    assert!(html.contains("src=\"/assets/photo.png\""), "{html}");
    let mut files: Vec<String> = std::fs::read_dir(site.path("public/assets"))
        .expect("assets")
        .map(|e| e.expect("entry").file_name().to_string_lossy().into_owned())
        .collect();
    files.sort();
    assert_eq!(files, vec!["photo.png".to_owned()], "one copy only");
}

/// A responsive variant is optimized like the source it was cut from. Encoding
/// a downscale and shipping it as written left a `srcset` whose middle
/// candidate was heavier than the optimized full-size image beside it.
#[test]
#[cfg(feature = "images")]
fn responsive_variants_are_optimized_like_their_source() {
    let config = |optimize: &str| {
        format!(
            "site \"T\"\npaths {{\n  content \"content\"\n  dist \"public\"\n  assets \"assets\"\n}}\nassets {{\n  images {{\n    responsive {{ widths 64 }}\n{optimize}  }}\n}}\n"
        )
    };
    let variant = |config: String| {
        let site = Site::with(&config);
        site.write_bytes("assets/photo.png", &png(128, 96));
        site.write(
            "content/index.typ",
            "#let frontmatter = (title: \"Home\",)\n#image(\"/assets/photo.png\")\n",
        );
        site.stats();
        std::fs::metadata(site.path("public/assets/photo-64.png"))
            .expect("the variant")
            .len()
    };
    let plain = variant(config(""));
    let optimized = variant(config("    optimize { png level=4 }\n"));
    assert!(
        optimized < plain,
        "variant was not optimized: {optimized} vs {plain}"
    );
}

/// An extracted image is recompressed like one in the asset tree: a picture
/// beside its page used to be the one unoptimized file on the site.
#[test]
#[cfg(feature = "images")]
fn a_colocated_image_is_optimized_on_the_way_out() {
    let site = Site::with(
        "site \"T\"\npaths {\n  content \"content\"\n  dist \"public\"\n}\nassets {\n  images {\n    extract #true\n    optimize { png level=4 strip=\"all\" }\n  }\n}\n",
    );
    let source = png(64, 48);
    site.write_bytes("content/photo.png", &source);
    site.write("content/index.typ", "#image(\"photo.png\")\n");
    site.stats();
    let written = std::fs::read(site.path("public/assets/photo.png")).expect("the copy");
    assert!(
        written.len() < source.len(),
        "copied unoptimized: {} vs {}",
        written.len(),
        source.len()
    );
}
