//! Image handling end to end: externalizing typst-embedded images out of the
//! DOM into files, responsive `srcset` variants, and the edge cases around both.

mod common;

use baudelaire::engine::{Engine, Mode};

use common::{Site, silent};

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

fn build(site: &Site) -> baudelaire::engine::Stats {
    Engine::new(site.config(), Mode::Build)
        .expect("engine")
        .build(&silent())
        .expect("build")
}

const EXTRACT: &str = "site \"T\"\npaths {\n  content \"content\"\n  dist \"public\"\n}\noutput {\n  images { extract #true }\n}\n";

#[test]
fn typst_image_externalizes_to_a_file() {
    let site = Site::with(EXTRACT);
    site.write_bytes("content/pic.png", &png(2, 2));
    site.write("content/index.typ", "#image(\"pic.png\")\n");

    build(&site);
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

#[test]
fn extract_is_the_default() {
    // Externalization is on out of the box: a bare config still writes the file.
    let site = Site::with("site \"T\"\npaths {\n  content \"content\"\n  dist \"public\"\n}\n");
    site.write_bytes("content/pic.png", &png(2, 2));
    site.write("content/index.typ", "#image(\"pic.png\")\n");

    build(&site);
    assert!(
        site.output("index.html")
            .contains("src=\"/assets/pic.png\"")
    );
    assert!(site.exists("public/assets/pic.png"));
}

#[test]
fn extract_false_keeps_typst_inlining() {
    // Opting out restores typst's native base64 inlining.
    let site = Site::with(
        "site \"T\"\npaths {\n  content \"content\"\n  dist \"public\"\n}\noutput {\n  images { extract #false }\n}\n",
    );
    site.write_bytes("content/pic.png", &png(2, 2));
    site.write("content/index.typ", "#image(\"pic.png\")\n");

    build(&site);
    let html = site.output("index.html");
    assert!(html.contains("data:image"), "image stays inline: {html}");
    assert!(!site.exists("public/assets/pic.png"));
}

#[test]
fn extract_preserves_typst_image_sizing() {
    // The override rule reproduces typst's width sizing as inline CSS, so an
    // `image(width: ..)` is not silently dropped when externalized.
    let site = Site::with(EXTRACT);
    site.write_bytes("content/pic.png", &png(2, 2));
    site.write("content/index.typ", "#image(\"pic.png\", width: 50%)\n");

    build(&site);
    let html = site.output("index.html");
    assert!(html.contains("width:50%"), "sizing is kept as CSS: {html}");
}

#[test]
fn fingerprint_hashes_the_externalized_name() {
    let site = Site::with(
        "site \"T\"\npaths {\n  content \"content\"\n  dist \"public\"\n}\noutput {\n  assets { fingerprint #true }\n  images { extract #true }\n}\n",
    );
    site.write_bytes("content/pic.png", &png(2, 2));
    site.write("content/index.typ", "#image(\"pic.png\")\n");

    build(&site);
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

    build(&site);
    let second = build(&site);
    assert!(second.cached >= 1, "the page was served from cache");
    assert!(
        site.exists("public/assets/pic.png"),
        "the file is present after a cached rebuild"
    );
}

#[test]
fn image_from_bytes_has_no_file_and_stays_inline() {
    // An image with no source path (raw bytes) cannot be externalized: there is
    // nothing on disk to reference, so it keeps its data URI even under extract.
    let site = Site::with(EXTRACT);
    site.write_bytes("content/pic.png", &png(2, 2));
    site.write(
        "content/index.typ",
        "#image(read(\"pic.png\", encoding: none))\n",
    );

    build(&site);
    let html = site.output("index.html");
    assert!(
        html.contains("data:image"),
        "bytes image stays inline: {html}"
    );
    assert!(!site.exists("public/assets/pic.png"));
}

/// Responsive variants are re-encoded by the image handler, which the `images`
/// feature owns; the slim flavour copies verbatim and emits no `srcset`.
#[test]
#[cfg(feature = "images")]
fn responsive_adds_srcset_and_writes_width_variants() {
    let site = Site::with(
        "site \"T\"\npaths {\n  content \"content\"\n  dist \"public\"\n}\noutput {\n  images {\n    responsive { widths 30 }\n  }\n}\n",
    );
    // A 60px-wide source asset, referenced by URL (the path externalization does
    // not cover): the pipeline generates a 30px variant and a srcset.
    site.write_bytes("assets/big.png", &png(60, 40));
    site.write(
        "content/index.typ",
        "#html.elem(\"img\", attrs: (src: \"/assets/big.png\"))\n",
    );

    build(&site);
    let html = site.output("index.html");
    assert!(html.contains("srcset="), "a srcset was added: {html}");
    assert!(
        html.contains("/assets/big-30.png 30w"),
        "the 30px variant is a candidate: {html}"
    );
    assert!(
        site.exists("public/assets/big-30.png"),
        "the variant file was written"
    );
    // No sizes attribute unless the config sets one (100vw is the browser default).
    assert!(!html.contains("sizes="), "no redundant sizes: {html}");
}

#[test]
fn responsive_skips_widths_at_or_above_the_source() {
    // A width no smaller than the source would upscale: it is dropped, and with
    // no smaller widths left, no srcset is produced.
    let site = Site::with(
        "site \"T\"\npaths {\n  content \"content\"\n  dist \"public\"\n}\noutput {\n  images {\n    responsive { widths 100 }\n  }\n}\n",
    );
    site.write_bytes("assets/small.png", &png(40, 40));
    site.write(
        "content/index.typ",
        "#html.elem(\"img\", attrs: (src: \"/assets/small.png\"))\n",
    );

    build(&site);
    let html = site.output("index.html");
    assert!(
        !html.contains("srcset="),
        "no srcset when nothing downscales: {html}"
    );
    assert!(!site.exists("public/assets/small-100.png"));
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
        "site \"T\"\npaths {\n  content \"content\"\n  dist \"public\"\n  assets \"assets\"\n}\noutput {\n  images {\n    optimize { png level=2 }\n    responsive { widths 20 }\n  }\n}\n",
    );
    site.write_bytes("assets/big.png", &png(60, 40));
    site.write(
        "content/index.typ",
        "#html.elem(\"img\", attrs: (src: \"/assets/big.png\"))\n",
    );
    build(&site);
    let first = std::fs::read(site.path("public/assets/big.png")).unwrap();
    let variant = std::fs::read(site.path("public/assets/big-20.png")).unwrap();

    // The memo survives the asset tree being regenerated wholesale.
    build(&site);
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
    build(&site);
    assert_ne!(
        std::fs::read(site.path("public/assets/big.png")).unwrap(),
        first,
        "an edited image must not come from the memo"
    );
}
