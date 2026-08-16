//! Image handling that turns on bytes rather than markup.
//!
//! Externalization, sizing and `srcset` are scenarios, in
//! `tests/scenarios/images.kdl`.

mod common;

use common::Site;

/// A tiny PNG of the given size, its pixels varying with position so two
/// different sizes never share bytes.
fn png(w: u32, h: u32) -> Vec<u8> {
    let img = image::RgbImage::from_fn(w, h, |x, y| {
        // The pattern is meant to wrap, so a truncating cast is the point.
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
fn same_named_images_in_different_directories_are_different_files() {
    let site = Site::with(EXTRACT);
    site.write_bytes("content/one/pic.png", &png(2, 2));
    site.write_bytes("content/two/pic.png", &png(3, 3));
    site.write("content/a.typ", "#image(\"one/pic.png\")\n");
    site.write("content/b.typ", "#image(\"two/pic.png\")\n");

    // The binary is spawned so a warning would surface on stderr.
    let stderr = site.build();
    assert!(
        !stderr.contains("two images map to"),
        "no collision to report: {stderr}"
    );
    assert!(site.exists("public/assets/one/pic.png"), "the first");
    assert!(site.exists("public/assets/two/pic.png"), "the second");
    assert!(
        site.output("a/index.html").contains("/assets/one/pic.png"),
        "each page points at its own image"
    );
    assert!(
        site.output("b/index.html").contains("/assets/two/pic.png"),
        "each page points at its own image"
    );
}

#[test]
fn an_extracted_image_never_overwrites_a_pipeline_asset() {
    let site = Site::with(
        "site \"T\"\npaths {\n  content \"content\"\n  dist \"public\"\n  assets \"assets\"\n}\nassets {\n  images {\n    extract #true\n  }\n}\n",
    );
    site.write_bytes("assets/one/pic.png", &png(2, 2));
    site.write_bytes("content/one/pic.png", &png(5, 5));
    site.write("content/a.typ", "#image(\"one/pic.png\")\n");

    let stderr = site.build();
    assert!(
        stderr.contains("two images map to") && stderr.contains("pic.png"),
        "collision warning surfaced: {stderr}"
    );
}

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

    site.write_bytes("assets/big.png", &png(60, 41));
    site.stats();
    assert_ne!(
        std::fs::read(site.path("public/assets/big.png")).unwrap(),
        first,
        "an edited image must not come from the memo"
    );
}

/// A page's `srcset` depends on the variants of the images it actually shows.
///
/// The image is resized rather than merely re-encoded, because that is what
/// moves the *manifest*: the recorded URLs are authored paths.
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

/// An image with no variants records that it had none, so gaining some later
/// still invalidates the page showing it.
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

#[test]
#[cfg(feature = "images")]
fn an_extracted_image_carries_its_own_srcset() {
    let site = Site::with(
        "site \"T\"\npaths {\n  content \"content\"\n  dist \"public\"\n}\nassets {\n  images {\n    extract #true\n    responsive { widths 32 64 128 }\n  }\n}\n",
    );
    site.write_bytes("content/photo.png", &png(96, 72));
    site.write("content/index.typ", "#image(\"photo.png\")\n");
    site.stats();

    let html = site.output("index.html");
    // 128 is at or above the source width, so it is not offered; the source
    // itself is the largest candidate.
    assert!(
        html.contains(
            "srcset=\"/assets/photo-32.png 32w, /assets/photo-64.png 64w, /assets/photo.png 96w\""
        ),
        "{html}"
    );
    assert!(site.exists("public/assets/photo-32.png"));
    assert!(site.exists("public/assets/photo-64.png"));
    assert!(
        !site.exists("public/assets/photo-128.png"),
        "never upscaled"
    );
}

/// A variant carries the *source's* digest, since the page names it before
/// those bytes exist.
#[test]
#[cfg(all(feature = "images", feature = "css"))]
fn a_fingerprinted_extracted_variant_is_named_as_the_page_promised() {
    let site = Site::with(
        "site \"T\"\npaths {\n  content \"content\"\n  dist \"public\"\n}\nassets {\n  fingerprint #true\n  images {\n    extract #true\n    responsive { widths 32 }\n  }\n}\n",
    );
    site.write_bytes("content/photo.png", &png(96, 72));
    site.write("content/index.typ", "#image(\"photo.png\")\n");
    site.stats();

    let html = site.output("index.html");
    let named: Vec<&str> = html
        .split('"')
        .flat_map(|chunk| chunk.split(", "))
        .filter(|c| c.contains("photo-32."))
        .collect();
    let url = named
        .first()
        .expect("a variant candidate")
        .trim_end_matches(" 32w");
    let file = url.trim_start_matches("/assets/");
    assert!(file.starts_with("photo-32."), "{url}");
    assert!(
        site.exists(&format!("public/assets/{file}")),
        "the page names a file that was written: {url}"
    );
}
