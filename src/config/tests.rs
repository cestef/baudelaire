use crate::config::{Config, ImageFormat, PngStrip, SortKey};

fn parse(text: &str) -> Config {
    Config::parse(text).expect("should parse")
}

#[test]
fn empty_uses_defaults() {
    let cfg = parse("");
    assert_eq!(cfg.lang, "en");
    assert!(cfg.clean);
    assert!(cfg.html.pretty);
    assert_eq!(cfg.serve.port, 3000);
    assert!(cfg.cache.incremental);
}

#[test]
fn scalars() {
    let cfg = parse(
        r#"
        site "Baudelaire"
        url "https://example.net"
        lang "fr"
        author "Claude"
        clean #false
        future #true
    "#,
    );
    assert_eq!(cfg.site.as_deref(), Some("Baudelaire"));
    assert_eq!(cfg.url.as_deref(), Some("https://example.net"));
    assert_eq!(cfg.lang, "fr");
    assert_eq!(cfg.author.as_deref(), Some("Claude"));
    assert!(!cfg.clean);
    assert!(cfg.future);
}

#[test]
fn inputs_and_features() {
    let cfg = parse(
        r#"
        typst {
          inputs {
            site "https://example.net"
            env  "prod"
          }
          features "+html" "-pdf"
        }
    "#,
    );
    assert_eq!(
        cfg.inputs,
        vec![
            ("site".into(), "https://example.net".into()),
            ("env".into(), "prod".into())
        ]
    );
    assert_eq!(cfg.features, vec!["html".to_owned(), "pdf".to_owned()]);
}

#[test]
fn collections_overrides() {
    let cfg = parse(
        r#"
        collections {
          posts "posts/**/*.typ" sort="date" reverse=#true permalink="/posts/{slug}/"
          notes "notes/**/*.typ" sort="order"
        }
    "#,
    );
    let posts = cfg.collections.iter().find(|(n, _)| n == "posts").unwrap();
    assert_eq!(posts.1.glob.as_deref(), Some("posts/**/*.typ"));
    assert_eq!(posts.1.sort, SortKey::Date);
    assert!(posts.1.reverse);
    assert_eq!(posts.1.permalink.as_deref(), Some("/posts/{slug}/"));
    let notes = cfg.collections.iter().find(|(n, _)| n == "notes").unwrap();
    assert_eq!(notes.1.sort, SortKey::Order);
    assert!(!notes.1.reverse);
}

#[test]
fn images_optimize_per_format_with_params_and_lax_extensions() {
    let cfg = parse(
        r#"
        output {
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
    assert!(!cfg.images.lazy);
    let opt = &cfg.images.optimize;
    let png = opt.png.as_ref().unwrap();
    assert_eq!(png.level, 4);
    assert_eq!(png.strip, PngStrip::All);
    assert_eq!(opt.jpeg.as_ref().unwrap().quality, 70);
    // Extension matching is lenient and case-insensitive.
    assert_eq!(opt.format("PNG"), Some(ImageFormat::Png));
    assert_eq!(opt.format("jpg"), Some(ImageFormat::Jpeg));
    assert_eq!(opt.format("jpeg"), Some(ImageFormat::Jpeg));
    assert_eq!(opt.format("gif"), None);
}

#[test]
fn images_optimize_defaults_when_empty() {
    let cfg = parse("output { images { optimize { png } } }");
    let png = cfg.images.optimize.png.as_ref().unwrap();
    assert_eq!(png.level, 2);
    assert_eq!(png.strip, PngStrip::Safe);
    // An unlisted format stays off.
    assert!(cfg.images.optimize.jpeg.is_none());
    assert!(cfg.images.lazy, "lazy defaults on");
}

#[test]
fn taxonomies() {
    let cfg = parse(
        r#"
        taxonomies {
          tags   index=#true
          series key="series" index=#false
        }
    "#,
    );
    let tags = cfg.taxonomies.iter().find(|(n, _)| n == "tags").unwrap();
    assert!(tags.1.index);
    let series = cfg.taxonomies.iter().find(|(n, _)| n == "series").unwrap();
    assert_eq!(series.1.key, "series");
    assert!(!series.1.index);
}

#[test]
fn html_pretty_toggle() {
    let cfg = parse("output {\n  html {\n    pretty #false\n  }\n}\n");
    assert!(!cfg.html.pretty);
}

#[test]
fn nested_parent_sections() {
    let cfg = parse(
        r#"
        paths {
          content "src"
          dist "out"
        }
        output {
          sitemap #false
          robots {
            disallow "/private/"
          }
          llms {
            summary "A test site."
          }
        }
    "#,
    );
    assert_eq!(cfg.content.to_str(), Some("src"));
    assert_eq!(cfg.dist.to_str(), Some("out"));
    assert!(!cfg.sitemap);
    assert!(cfg.robots.enabled);
    assert_eq!(cfg.robots.disallow, vec!["/private/".to_owned()]);
    assert!(cfg.llms.enabled);
    assert_eq!(cfg.llms.summary.as_deref(), Some("A test site."));
}

#[test]
fn err_unknown_key_in_parent_section() {
    let err = Config::parse("paths {\n  bogus \"x\"\n}\n").unwrap_err();
    assert!(err.to_string().contains("unknown key `bogus`"), "{err}");
}

#[test]
fn serve_overrides() {
    let cfg = parse(
        r#"
        serve {
          port 8080
          bind "0.0.0.0"
          open #false
          watch #false
        }
    "#,
    );
    assert_eq!(cfg.serve.port, 8080);
    assert_eq!(cfg.serve.bind, "0.0.0.0");
    assert!(!cfg.serve.open);
    assert!(!cfg.serve.watch);
}

#[test]
fn err_unknown_top_key() {
    let err = Config::parse("bogus \"\"").unwrap_err();
    assert!(err.to_string().contains("unknown key `bogus`"));
}

#[test]
fn err_bad_sort_key() {
    let err = Config::parse("collections {\n  posts sort=\"wat\"\n}\n").unwrap_err();
    assert!(err.to_string().contains("unknown sort key `wat`"));
}

#[test]
fn err_missing_arg() {
    let err = Config::parse("site").unwrap_err();
    assert!(err.to_string().contains("missing argument"));
}

#[test]
fn err_missing_children() {
    let err = Config::parse("serve").unwrap_err();
    assert!(err.to_string().contains("missing children"));
}
