//! `assets { images { } }`: optimization, extraction, responsive variants.

use super::parse;
use crate::config::{Config, PngStrip};
use crate::mime::ImageFormat;
#[test]
fn images_optimize_per_format_with_params_and_lax_extensions() {
    let cfg = parse(
        r#"
        assets {
          images {
            lazy #false
            optimize {
              png level=4 strip="all"
              jpeg quality=70
            }
          }
        }
    "#,
    );
    assert!(!cfg.assets.images.lazy);
    let opt = &cfg.assets.images.optimize;
    let png = opt.png.as_ref().unwrap();
    assert_eq!(png.level, 4);
    assert_eq!(png.strip, PngStrip::All);
    assert_eq!(opt.jpeg.as_ref().unwrap().quality, 70);
    assert_eq!(opt.format("PNG"), Some(ImageFormat::Png));
    assert_eq!(opt.format("jpg"), Some(ImageFormat::Jpeg));
    assert_eq!(opt.format("jpeg"), Some(ImageFormat::Jpeg));
    assert_eq!(opt.format("gif"), None);
}

/// One config key per format, file *extensions* being a different table and
/// staying lenient.
#[test]
fn images_optimize_names_each_format_once() {
    let err = Config::parse("assets { images { optimize { jpg quality=70 } } }").unwrap_err();
    let rendered = format!("{:?}", miette::Report::from(err));
    assert!(rendered.contains("unknown config key `jpg`"), "{rendered}");
    assert!(rendered.contains("did you mean `jpeg`?"), "{rendered}");
    let opt = &parse("assets { images { optimize { jpeg } } }")
        .assets
        .images
        .optimize;
    assert_eq!(opt.format("jpg"), Some(ImageFormat::Jpeg));
}

#[test]
fn err_unknown_image_format_suggests_a_valid_one() {
    let err = Config::parse("assets { images { optimize { pgn } } }").unwrap_err();
    let rendered = format!("{:?}", miette::Report::from(err));
    assert!(rendered.contains("unknown config key `pgn`"), "{rendered}");
    assert!(rendered.contains("did you mean `png`?"), "{rendered}");
}

#[test]
fn images_optimize_defaults_when_empty() {
    let cfg = parse("assets { images { optimize { png } } }");
    let png = cfg.assets.images.optimize.png.as_ref().unwrap();
    assert_eq!(png.level, 2);
    assert_eq!(png.strip, PngStrip::Safe);
    assert!(cfg.assets.images.optimize.jpeg.is_none());
    assert!(cfg.assets.images.lazy, "lazy defaults on");
}

#[test]
fn images_extract_defaults_on_and_parses() {
    assert!(parse("").assets.images.extract);
    let cfg = parse("assets {\n  images {\n    extract #false\n  }\n}\n");
    assert!(!cfg.assets.images.extract);
}

#[test]
fn externalize_gate_yields_to_embed() {
    let extract = parse("assets {\n  images { extract #true }\n}\n");
    assert!(extract.assets.images.externalize(&extract.html));
    let both = parse("html { embed #true }\nassets {\n  images { extract #true }\n}\n");
    assert!(!both.assets.images.externalize(&both.html));
}

#[test]
fn responsive_block_enables_with_default_widths() {
    assert!(!parse("").assets.images.responsive.enabled);
    let cfg = parse("assets {\n  images {\n    responsive { }\n  }\n}\n");
    assert!(cfg.assets.images.responsive.enabled);
    assert_eq!(cfg.assets.images.responsive.widths, vec![480, 960, 1440]);
    assert_eq!(cfg.assets.images.responsive.quality, 80);
    assert!(cfg.assets.images.responsive.sizes.is_none());
}

#[test]
fn responsive_widths_and_sizes_override() {
    let cfg = parse(
        "assets {\n  images {\n    responsive {\n      widths 320 640\n      quality 70\n      sizes \"50vw\"\n    }\n  }\n}\n",
    );
    assert_eq!(cfg.assets.images.responsive.widths, vec![320, 640]);
    assert_eq!(cfg.assets.images.responsive.quality, 70);
    assert_eq!(cfg.assets.images.responsive.sizes.as_deref(), Some("50vw"));
}

#[test]
fn responsive_rejects_a_zero_width() {
    assert!(Config::parse("assets {\n  images {\n    responsive { widths 0 }\n  }\n}\n").is_err());
}
