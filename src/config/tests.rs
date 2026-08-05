use miette::Diagnostic;

use crate::config::{Config, FieldType, PngStrip, SortKey};
use crate::error::BaudelaireErrorKind;
use crate::mime::ImageFormat;

fn parse(text: &str) -> Config {
    Config::parse(text).expect("should parse")
}

#[test]
fn languages_block_parses_with_names_and_strings() {
    let cfg = parse(
        r#"
        lang "en"
        languages {
            fr { name "Français"; strings { more "Lire la suite" } }
            de { name "Deutsch"; dir "ltr" }
        }
    "#,
    );
    assert_eq!(cfg.languages.len(), 2);
    let (code, fr) = &cfg.languages[0];
    assert_eq!(code, "fr");
    assert_eq!(fr.name.as_deref(), Some("Français"));
    assert_eq!(fr.strings.len(), 1);
}

#[test]
fn langs_lists_default_first_then_declared() {
    let cfg = parse("lang \"en\"\nlanguages {\n  fr { }\n  de { }\n}\n");
    assert_eq!(cfg.langs(), ["en", "fr", "de"]);
    assert!(cfg.knows("fr") && cfg.knows("en") && !cfg.knows("es"));
    assert!(cfg.multilingual());
}

#[test]
fn localize_prefixes_only_non_default_languages() {
    let cfg = parse("lang \"en\"\nlanguages {\n  fr { }\n}\n");
    assert_eq!(cfg.localize("en", "/posts/hello/"), "/posts/hello/");
    assert_eq!(cfg.localize("fr", "/posts/hello/"), "/fr/posts/hello/");
    assert_eq!(cfg.localize("fr", "/"), "/fr/");
}

#[test]
fn client_block_parses_json_scalars() {
    use crate::codegen::Value;
    let cfg = parse("client {\n  env \"prod\"\n  retries 3\n  ratio 0.5\n  beta #true\n}\n");
    let map: std::collections::BTreeMap<_, _> = cfg.client.iter().cloned().collect();
    assert_eq!(map["env"], Value::str("prod"));
    assert_eq!(map["retries"], Value::Int(3));
    assert_eq!(map["ratio"], Value::Float(0.5));
    assert_eq!(map["beta"], Value::Bool(true));
}

#[test]
fn empty_uses_defaults() {
    let cfg = parse("");
    assert_eq!(cfg.lang, "en");
    assert_eq!(cfg.links.style, crate::config::UrlStyle::Clean);
    assert!(cfg.prune);
    assert!(cfg.html.pretty);
    assert_eq!(cfg.serve.port, 1821);
    assert!(cfg.cache.incremental);
}

#[test]
fn announce_standard_block_enables_backend_with_defaults() {
    let cfg = parse("announce {\n  standard {\n    handle \"me.bsky.social\"\n  }\n}\n");
    let standard = cfg.announce.standard.expect("standard backend configured");
    assert_eq!(standard.handle, "me.bsky.social");
    assert_eq!(standard.pds, "https://bsky.social");
    assert!(standard.discover);
    assert!(standard.icon.is_none());
}

#[test]
fn announce_unset_leaves_no_backend() {
    assert!(parse("").announce.standard.is_none());
}

#[test]
fn announce_standard_did_and_verify_toggles() {
    let cfg = parse(
        "announce {\n  standard {\n    handle \"me.example\"\n    did \"did:plc:abc\"\n    verify {\n      links #false\n    }\n  }\n}\n",
    );
    let standard = cfg.announce.standard.expect("configured");
    assert_eq!(standard.did.as_deref(), Some("did:plc:abc"));
    // toggled off explicitly; the untouched sibling keeps its default
    assert!(!standard.verify.links);
    assert!(standard.verify.wellknown);
}

#[test]
fn bundle_index_defaults_to_index_and_is_configurable() {
    assert_eq!(parse("").content.index.as_deref(), Some("index"));
    assert_eq!(
        parse("content {\n  index \"_index\"\n}")
            .content
            .index
            .as_deref(),
        Some("_index")
    );
    // an empty basename disables bundle slugs
    assert_eq!(parse("content {\n  index \"\"\n}").content.index, None);
}

#[test]
fn scalars() {
    let cfg = parse(
        r#"
        site "Baudelaire"
        url "https://example.net"
        lang "fr"
        author "Claude"
        content { future #true }
    "#,
    );
    assert_eq!(cfg.site.as_deref(), Some("Baudelaire"));
    assert_eq!(cfg.url.as_deref(), Some("https://example.net"));
    assert_eq!(cfg.lang, "fr");
    assert_eq!(cfg.author.as_deref(), Some("Claude"));
    assert!(cfg.content.future);
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
          features "+html" "pdf"
        }
    "#,
    );
    assert_eq!(
        cfg.typst.inputs,
        vec![
            ("site".into(), "https://example.net".into()),
            ("env".into(), "prod".into())
        ]
    );
    assert_eq!(
        cfg.typst.features,
        vec!["html".to_owned(), "pdf".to_owned()]
    );
}

#[test]
fn feature_disable_parses_as_negated_token() {
    let cfg = parse("typst {\n  features \"+a11y-extras\" \"-bundle\"\n}\n");
    assert_eq!(
        cfg.typst.features,
        vec!["a11y-extras".to_owned(), "-bundle".to_owned()]
    );
}

/// A mirror is stored ready to be joined onto, so the store never builds a
/// `//preview/..` URL out of a trailing slash the author is entitled to write.
#[test]
fn registry_mirror_drops_its_trailing_slash() {
    let cfg = parse("typst {\n  registry \"https://packages.example.net/\"\n}\n");
    assert_eq!(
        cfg.typst.registry.as_deref(),
        Some("https://packages.example.net")
    );
    assert_eq!(Config::default().typst.registry, None);
}

/// `url` is what every absolute URL the site emits is built by concatenation
/// onto, so a value that is not one produced `<loc>example.com/a/</loc>` in the
/// sitemap, the same in every feed `<id>` and canonical tag, out of a green
/// build.
#[test]
fn err_a_site_url_without_a_scheme_is_refused() {
    for bad in [
        "example.com",
        "//example.com",
        "https://",
        "https://a b.com",
    ] {
        let err = Config::parse(&format!("url {bad:?}")).unwrap_err();
        let rendered = format!("{:?}", miette::Report::from(err));
        assert!(
            rendered.contains("is not an absolute URL"),
            "{bad}: {rendered}"
        );
    }
}

/// ...and any scheme is the author's business: `url` is served to readers, not
/// a host credentials are sent to. That is `typst { registry }`'s rule, not
/// this one's.
#[test]
fn a_site_url_may_name_any_scheme() {
    for good in [
        "https://example.com",
        "http://example.com",
        "http://localhost:8080/docs",
        "https://example.com/docs/",
    ] {
        assert_eq!(
            parse(&format!("url {good:?}")).url.as_deref(),
            Some(good),
            "{good} should parse"
        );
    }
}

/// Package tarballs are code the build executes, so a plaintext mirror is
/// refused exactly like every other URL the config accepts.
#[test]
fn err_insecure_registry_rejected() {
    let err =
        Config::parse("typst {\n  registry \"http://packages.example.net\"\n}\n").unwrap_err();
    assert!(err.to_string().contains("https"), "{err}");
}

#[test]
fn err_html_feature_removal_rejected() {
    let err = Config::parse("typst {\n  features \"-html\"\n}\n").unwrap_err();
    assert!(
        err.to_string()
            .contains("feature `html` is required and cannot be disabled"),
        "{err}"
    );
}

#[test]
fn collections_overrides() {
    let cfg = parse(
        r#"
        content {
          collections {
            posts "posts/**/*.typ" {
              sort "date"
              reverse #true
              permalink "/posts/{slug}/"
            }
            notes "notes/**/*.typ" { sort "order" }
          }
        }
    "#,
    );
    let posts = cfg
        .content
        .collections
        .iter()
        .find(|(n, _)| n == "posts")
        .unwrap();
    assert_eq!(posts.1.glob.as_deref(), Some("posts/**/*.typ"));
    assert_eq!(posts.1.sort, SortKey::Date);
    assert!(posts.1.reverse);
    assert_eq!(posts.1.permalink.as_deref(), Some("/posts/{slug}/"));
    let notes = cfg
        .content
        .collections
        .iter()
        .find(|(n, _)| n == "notes")
        .unwrap();
    assert_eq!(notes.1.sort, SortKey::Order);
    assert!(!notes.1.reverse);
}

/// `glob` is the field's name in the docs and in the struct, and it is now the
/// parser's too. It used to be positional-only, so writing the documented thing
/// failed with a help that listed every key except the one being written.
#[test]
fn a_collection_glob_can_be_named_as_well_as_positional() {
    let cfg = parse(r#"content { collections { posts { glob "p/**/*.typ" } } }"#);
    let posts = &cfg.content.collections[0].1;
    assert_eq!(posts.glob.as_deref(), Some("p/**/*.typ"));

    // The named form is read after the positional, so a line writing both takes
    // the one it spelled out.
    let cfg = parse(r#"content { collections { posts "a/*.typ" { glob "b/*.typ" } } }"#);
    assert_eq!(
        cfg.content.collections[0].1.glob.as_deref(),
        Some("b/*.typ")
    );
}

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
    // extension matching is lenient and case-insensitive
    assert_eq!(opt.format("PNG"), Some(ImageFormat::Png));
    assert_eq!(opt.format("jpg"), Some(ImageFormat::Jpeg));
    assert_eq!(opt.format("jpeg"), Some(ImageFormat::Jpeg));
    assert_eq!(opt.format("gif"), None);
}

/// One config key per format. `jpg` was a second key onto the same field, so a
/// block naming both configured one format twice with no duplicate diagnostic,
/// and the "valid keys" help listed them as if they were different formats.
/// File *extensions* stay lenient: that is a different table, and `photo.jpg`
/// is what people name files.
#[test]
fn images_optimize_names_each_format_once() {
    let err = Config::parse("assets { images { optimize { jpg quality=70 } } }").unwrap_err();
    let rendered = format!("{:?}", miette::Report::from(err));
    assert!(rendered.contains("unknown config key `jpg`"), "{rendered}");
    assert!(rendered.contains("did you mean `jpeg`?"), "{rendered}");
    // The *extension* table stays lenient: `photo.jpg` is what people name
    // files, and that is a different table.
    let opt = &parse("assets { images { optimize { jpeg } } }")
        .assets
        .images
        .optimize;
    assert_eq!(opt.format("jpg"), Some(ImageFormat::Jpeg));
}

/// A scope whose settings are `key=value` on one line refuses a `{ }` block.
///
/// It used to accept one and read nothing out of it: the block parsed, the
/// build went green, and the setting inside was simply not applied. The
/// generated reference called these "block" at the time, so it documented the
/// spelling that did nothing.
#[test]
fn err_a_block_on_an_attribute_scope_is_refused() {
    for (config, node) in [
        ("assets { images { optimize { png { level 6 } } } }", "png"),
        (
            "assets { images { optimize { jpeg { quality 70 } } } }",
            "jpeg",
        ),
        ("content { taxonomies { tags { listing #true } } }", "tags"),
        (
            r#"generate { manifest { icons { "/i.png" { size 512 } } } }"#,
            "/i.png",
        ),
    ] {
        let err = Config::parse(config).unwrap_err();
        let rendered = format!("{:?}", miette::Report::from(err));
        assert!(
            rendered.contains(&format!("`{node}` takes no block")),
            "{rendered}"
        );
        assert!(rendered.contains("write them as attributes"), "{rendered}");
    }
}

/// The one attribute scope that does take a block: a `dict` field's own fields.
#[test]
fn a_schema_dict_still_declares_its_fields_in_a_block() {
    let cfg =
        parse(r#"content { collections { posts { schema { author "dict" { name "str" } } } } }"#);
    let (key, field) = &cfg.content.collections[0].1.schema[0];
    assert_eq!(key, "author");
    let FieldType::Dict(fields) = &field.ty else {
        panic!("the dict's own fields were not read from its block");
    };
    assert_eq!(fields[0].0, "name");
}

/// An unrecognized format reads like every other unknown key, suggestions
/// included, because the same table drives parsing and the error.
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
    // an unlisted format stays off
    assert!(cfg.assets.images.optimize.jpeg.is_none());
    assert!(cfg.assets.images.lazy, "lazy defaults on");
}

#[test]
fn taxonomies() {
    let cfg = parse(
        r#"
        content {
          taxonomies {
            tags   listing=#true
            series key="series" listing=#false
          }
        }
    "#,
    );
    let tags = cfg
        .content
        .taxonomies
        .iter()
        .find(|(n, _)| n == "tags")
        .unwrap();
    assert!(tags.1.listing);
    let series = cfg
        .content
        .taxonomies
        .iter()
        .find(|(n, _)| n == "series")
        .unwrap();
    assert_eq!(series.1.key, "series");
    assert!(!series.1.listing);
}

#[test]
fn html_pretty_toggle() {
    let cfg = parse("html {\n  pretty #false\n}\n");
    assert!(!cfg.html.pretty);
}

#[test]
fn nested_parent_sections() {
    let cfg = parse(
        r#"
        paths {
          content "src"
          dist "out"
          static "public-files"
        }
        generate {
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
    assert_eq!(cfg.paths.content.to_str(), Some("src"));
    assert_eq!(cfg.paths.dist.to_str(), Some("out"));
    assert_eq!(cfg.paths.r#static.to_str(), Some("public-files"));
    assert!(!cfg.generate.sitemap);
    assert!(cfg.client.is_empty());
    assert!(cfg.generate.robots.enabled);
    assert_eq!(cfg.generate.robots.disallow, vec!["/private/".to_owned()]);
    assert!(cfg.generate.llms.enabled);
    assert_eq!(cfg.generate.llms.summary.as_deref(), Some("A test site."));
}

#[test]
fn err_unknown_key_in_parent_section() {
    let err = Config::parse("paths {\n  bogus \"x\"\n}\n").unwrap_err();
    assert!(
        err.to_string().contains("unknown config key `bogus`"),
        "{err}"
    );
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
    assert!(err.to_string().contains("unknown config key `bogus`"));
}

#[test]
fn err_bad_sort_key() {
    let err = Config::parse("content {\n  collections {\n    posts { sort \"wat\" }\n  }\n}\n")
        .unwrap_err();
    let rendered = format!("{:?}", miette::Report::from(err));
    // an unknown enum *value* reads as "unknown value", not "unknown key"
    assert!(rendered.contains("unknown value `wat`"), "{rendered}");
    assert!(rendered.contains("`order`, `date`, `title`"), "{rendered}");
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

#[test]
fn bare_flag_node_enables() {
    // A bare flag node enables; the default is on too.
    assert!(parse("").prune);
    assert!(parse("prune").prune);
    assert!(!parse("prune #false").prune);
}

#[test]
fn err_boolean_type() {
    let err = Config::parse("prune \"yes\"").unwrap_err();
    assert!(
        err.to_string().contains("expected boolean, got string"),
        "{err}"
    );
}

#[test]
fn url_style_parsed_from_links_block() {
    use crate::config::UrlStyle;
    assert_eq!(
        parse("links {\n  style \"clean\"\n}").links.style,
        UrlStyle::Clean
    );
    assert_eq!(
        parse("links {\n  style \"flat\"\n}").links.style,
        UrlStyle::Flat
    );
}

#[test]
fn err_unknown_url_style() {
    let err = Config::parse("links {\n  style \"pretty\"\n}").unwrap_err();
    let rendered = format!("{:?}", miette::Report::from(err));
    // The value table drives the "did you mean" hint, listing the valid styles.
    assert!(rendered.contains("unknown value `pretty`"), "{rendered}");
    assert!(rendered.contains("`clean`, `flat`"), "{rendered}");
}

#[test]
fn err_boolean_attr_type() {
    let err = Config::parse("content {\n  collections {\n    posts { reverse \"yes\" }\n  }\n}\n")
        .unwrap_err();
    assert!(
        err.to_string().contains("expected boolean, got string"),
        "{err}"
    );
}

#[test]
fn err_port_out_of_range() {
    let err = Config::parse("serve {\n  port 99999\n}\n").unwrap_err();
    assert!(
        err.to_string().contains("port must be 0-65535, got 99999"),
        "{err}"
    );
}

#[test]
fn err_integer_overflows_i64() {
    let err = Config::parse("serve {\n  port 99999999999999999999\n}\n").unwrap_err();
    assert!(err.to_string().contains("out of range"), "{err}");
}

#[test]
fn err_negative_limit_and_minimum() {
    let err = Config::parse("generate {\n  feed {\n    limit -1\n  }\n}\n").unwrap_err();
    assert!(
        err.to_string()
            .contains("`limit` must not be negative, got -1"),
        "{err}"
    );
    let err = Config::parse("generate {\n  search {\n    minimum -2\n  }\n}\n").unwrap_err();
    assert!(
        err.to_string()
            .contains("`minimum` must not be negative, got -2"),
        "{err}"
    );
}

#[test]
fn err_paginate_below_one() {
    for (config, detail) in [
        (
            "content {\n  collections {\n    posts { paginate { size 0 } }\n  }\n}\n",
            "paginate must be at least 1, got 0",
        ),
        (
            "content {\n  collections {\n    posts { paginate { size -3 } }\n  }\n}\n",
            "paginate must be at least 1, got -3",
        ),
    ] {
        let err = Config::parse(config).unwrap_err();
        assert!(err.to_string().contains(detail), "{err}");
    }
}

#[test]
fn err_duplicate_format() {
    let err =
        Config::parse("generate {\n  feed {\n    formats \"rss\" \"rss\"\n  }\n}\n").unwrap_err();
    assert!(
        err.to_string().contains("duplicate `rss` in `formats`"),
        "{err}"
    );
}

#[test]
fn err_duplicate_collection() {
    let err = Config::parse(
        "content {\n  collections {\n    posts\n    posts { sort \"date\" }\n  }\n}\n",
    )
    .unwrap_err();
    assert!(
        err.to_string().contains("duplicate collection `posts`"),
        "{err}"
    );
}

#[test]
fn err_duplicate_taxonomy() {
    let err =
        Config::parse("content {\n  taxonomies {\n    tags\n    tags listing=#true\n  }\n}\n")
            .unwrap_err();
    assert!(
        err.to_string().contains("duplicate taxonomy `tags`"),
        "{err}"
    );
}

#[test]
fn err_duplicate_profile() {
    let err = Config::parse("profiles {\n  dev { prune #true }\n  dev { prune #false }\n}\n")
        .unwrap_err();
    assert!(err.to_string().contains("duplicate profile `dev`"), "{err}");
}

#[test]
fn err_unknown_permalink_placeholder_is_spanned() {
    let text = "content {\n  collections {\n    posts { permalink \"/{bogus}/\" }\n  }\n}\n";
    let err = Config::parse(text).unwrap_err();
    let rendered = format!("{:?}", miette::Report::from(err));
    assert!(
        rendered.contains("unknown permalink placeholder `bogus`"),
        "{rendered}"
    );
    assert!(rendered.contains("valid placeholders"), "{rendered}");
    // the label excerpts config.kdl: the error is spanned at parse time
    assert!(rendered.contains("permalink "), "{rendered}");
}

#[test]
fn err_unterminated_permalink_placeholder() {
    let err =
        Config::parse("content {\n  collections {\n    posts { permalink \"/{slug\" }\n  }\n}\n")
            .unwrap_err();
    assert!(err.to_string().contains("unterminated `{slug`"), "{err}");
}

#[test]
fn err_permalink_parent_dir_segment() {
    let err = Config::parse(
        "content {\n  collections {\n    posts { permalink \"/../{slug}/\" }\n  }\n}\n",
    )
    .unwrap_err();
    assert!(err.to_string().contains("must not contain `..`"), "{err}");
}

#[test]
fn err_unexpected_positional_argument() {
    // taxonomies take no positional arguments
    let err = Config::parse("content {\n  taxonomies {\n    tags \"extra\"\n  }\n}\n").unwrap_err();
    assert!(err.to_string().contains("unexpected argument"), "{err}");
    // collections consume exactly one (the glob); a second is discarded today
    let err =
        Config::parse("content {\n  collections {\n    posts \"posts/*.typ\" \"extra\"\n  }\n}\n")
            .unwrap_err();
    assert!(err.to_string().contains("unexpected argument"), "{err}");
}

#[test]
fn err_unset_env_var_without_default() {
    let err = Config::parse("site \"${BAUDELAIRE_TEST_UNSET_VAR}\"").unwrap_err();
    let rendered = format!("{:?}", miette::Report::from(err));
    assert!(
        rendered.contains("environment variable `BAUDELAIRE_TEST_UNSET_VAR` is not set"),
        "{rendered}"
    );
    assert!(rendered.contains(":-default"), "{rendered}");
}

#[test]
fn destination_never_escapes_dist() {
    let cfg = parse("");
    let dist = cfg.paths.dist.clone();
    for url in ["/../../etc/passwd/", "/posts/../../secret/"] {
        let written = cfg.destination(url);
        assert!(written.starts_with(&dist), "{url} -> {}", written.display());
        assert!(
            !written.components().any(|c| c.as_os_str() == ".."),
            "{url} -> {}",
            written.display()
        );
    }
}

/// Values carrying a `codegen::Value` reach the cache fingerprint through a
/// real `Hash`. Hashing their serialization instead meant any two configs whose
/// serialization failed fingerprinted identically, and a stale site.
#[test]
fn client_and_language_values_key_the_fingerprint() {
    use crate::graph::Hash;
    let base = parse("client {\n  env \"prod\"\n}\nlanguages {\n  fr { name \"Français\" }\n}");
    let client = parse("client {\n  env \"dev\"\n}\nlanguages {\n  fr { name \"Français\" }\n}");
    let language = parse("client {\n  env \"prod\"\n}\nlanguages {\n  fr { name \"Francais\" }\n}");
    assert_ne!(Hash::of(&base), Hash::of(&client));
    assert_ne!(Hash::of(&base), Hash::of(&language));
}

/// A translated not-found page must stay a flat `404.html` inside its language
/// scope: `dist/fr/404/index.html` is not served as not-found by any host.
#[test]
fn a_localized_404_stays_a_flat_page() {
    let cfg = parse("lang \"en\"\nlanguages {\n  fr { }\n}");
    assert_eq!(cfg.destination("/404/"), cfg.paths.dist.join("404.html"));
    assert_eq!(
        cfg.destination("/fr/404/"),
        cfg.paths.dist.join("fr/404.html")
    );
    // Only a language scope counts: an ordinary page that happens to be called
    // `404` keeps the site's URL style.
    assert_eq!(
        cfg.destination("/notes/404/"),
        cfg.paths.dist.join("notes/404/index.html")
    );
    assert_eq!(
        cfg.destination("/posts/4040/"),
        cfg.paths.dist.join("posts/4040/index.html")
    );
}

#[test]
fn deploy_s3_block_enables_backend_with_defaults() {
    let cfg = parse(
        "deploy {\n  s3 {\n    bucket \"my-site\"\n    endpoint \"https://acct.r2.cloudflarestorage.com\"\n    region \"auto\"\n  }\n}\n",
    );
    let s3 = cfg.deploy.s3.expect("s3 backend configured");
    assert_eq!(s3.bucket, "my-site");
    assert_eq!(
        s3.endpoint.as_deref(),
        Some("https://acct.r2.cloudflarestorage.com")
    );
    assert_eq!(s3.region(), "auto");
    assert_eq!(s3.prefix, "");
    assert!(s3.delete, "delete defaults on");
}

/// An unstated region follows the target. A custom `endpoint` is not AWS, and
/// signing such a request as `us-east-1` is a 403 whose body never mentions the
/// region; AWS itself keeps its own default.
#[test]
fn an_unstated_s3_region_follows_the_endpoint() {
    let r2 =
        parse("deploy { s3 { bucket \"b\"; endpoint \"https://acct.r2.cloudflarestorage.com\" } }");
    assert_eq!(r2.deploy.s3.unwrap().region(), "auto");
    let aws = parse("deploy { s3 { bucket \"b\" } }");
    assert_eq!(aws.deploy.s3.unwrap().region(), "us-east-1");
}

#[test]
fn deploy_ssh_block_enables_backend_with_defaults() {
    let cfg = parse(
        "deploy {\n  ssh {\n    host \"example.com\"\n    path \"/var/www/site\"\n    user \"deploy\"\n  }\n}\n",
    );
    let ssh = cfg.deploy.ssh.expect("ssh backend configured");
    assert_eq!(ssh.host, "example.com");
    assert_eq!(ssh.path, "/var/www/site");
    assert_eq!(ssh.user.as_deref(), Some("deploy"));
    assert_eq!(ssh.port, 22, "port defaults to 22");
    assert!(ssh.key.is_none());
    assert!(ssh.strict, "host-key verification defaults on");
    assert!(ssh.delete, "delete defaults on");
}

#[test]
fn deploy_unset_leaves_no_backend() {
    let deploy = parse("").deploy;
    assert!(deploy.s3.is_none());
    assert!(deploy.ssh.is_none());
}

#[test]
fn base_path_is_the_url_path_component() {
    assert_eq!(parse("url \"https://host.test/docs\"").base_path(), "/docs");
    assert_eq!(parse("url \"https://host.test/a/b\"").base_path(), "/a/b");
    assert_eq!(
        parse("url \"https://host.test/docs/\"").base_path(),
        "/docs"
    );
    assert_eq!(parse("url \"https://host.test\"").base_path(), "");
    assert_eq!(parse("url \"https://host.test/\"").base_path(), "");
    assert_eq!(parse("").base_path(), "");
}

#[test]
fn prefixed_shifts_only_root_absolute_paths() {
    let cfg = parse("url \"https://host.test/docs\"");
    assert_eq!(cfg.prefixed("/guide/"), "/docs/guide/");
    assert_eq!(cfg.prefixed("//cdn/x"), "//cdn/x");
    assert_eq!(cfg.prefixed("https://x/y"), "https://x/y");
    assert_eq!(cfg.prefixed("#frag"), "#frag");
    assert_eq!(
        parse("url \"https://host.test\"").prefixed("/guide/"),
        "/guide/"
    );
}

#[test]
fn images_extract_defaults_on_and_parses() {
    assert!(parse("").assets.images.extract);
    let cfg = parse("assets {\n  images {\n    extract #false\n  }\n}\n");
    assert!(!cfg.assets.images.extract);
}

#[test]
fn externalize_gate_yields_to_embed() {
    // `extract` alone externalizes; `html.embed` (which re-inlines assets)
    // overrides it so the two never fight.
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
    // silent block keeps the built-in breakpoints and quality.
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
    // widths are 1..=16384; a 0 (or negative) is a hard error, not a silent drop.
    assert!(Config::parse("assets {\n  images {\n    responsive { widths 0 }\n  }\n}\n").is_err());
}

/// A monolingual right-to-left site has no `languages` block to declare `dir`
/// in, so the direction has to come from the code itself.
#[test]
fn rtl_is_inferred_when_no_language_declares_it() {
    assert_eq!(parse("lang \"ar\"").dir("ar"), Some("rtl"));
    assert_eq!(parse("lang \"az-Arab\"").dir("az-Arab"), Some("rtl"));
    assert_eq!(parse("lang \"en\"").dir("en"), None);
    assert_eq!(parse("lang \"fr\"").dir("fr"), None);
}

/// An explicit `dir` still wins over the inference.
#[test]
fn a_declared_dir_overrides_the_inferred_one() {
    let cfg = parse("lang \"en\"\nlanguages {\n  ar { dir \"ltr\" }\n}");
    assert_eq!(cfg.dir("ar"), Some("ltr"));
}

/// Absolute URLs are percent-encoded: `<loc>` and a feed's `<id>` are specified
/// as URIs, and slugs now carry raw UTF-8.
#[test]
fn absolute_urls_are_percent_encoded() {
    let cfg = parse("url \"https://host.test\"");
    let base = cfg.base().expect("base");
    assert_eq!(
        base.join("/posts/café/"),
        "https://host.test/posts/caf%C3%A9/"
    );
    // Path structure and the sub-delimiters a URL legitimately uses survive.
    assert_eq!(base.join("/a/b-c_d~e/"), "https://host.test/a/b-c_d~e/");
    // An already-encoded path is not encoded twice.
    assert_eq!(
        base.join("/posts/caf%C3%A9/"),
        "https://host.test/posts/caf%C3%A9/"
    );
}

/// ...and decoding is its inverse, which is what the dev server applies to an
/// incoming request path.
#[test]
fn percent_round_trips() {
    use crate::config::Percent;
    for path in ["/posts/café/", "/a b/", "/日本語/", "/plain/"] {
        assert_eq!(Percent::decode(&Percent::encode(path)), path);
    }
}

/// A `dist` that contains the sources is what `paths { dist "." }` produced: the
/// prune sweep deleted `config.kdl`, the content tree and every unrelated file in
/// the project, and reported a successful build.
#[test]
fn dist_containing_a_source_directory_is_refused() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path();

    let cfg = parse("paths { dist \".\" }");
    let (key, _) = cfg.paths.swallowed(root).expect("`.` swallows the project");
    assert_eq!(key, "content");

    // Nesting one level down is the same hazard spelled less obviously.
    let cfg = parse("paths { dist \"out\"; content \"out/content\" }");
    let (key, _) = cfg.paths.swallowed(root).expect("nested content swallowed");
    assert_eq!(key, "content");

    // As is a `dist` above the project, which reaches the sources by climbing.
    let cfg = parse("paths { dist \"..\" }");
    assert!(cfg.paths.swallowed(root).is_some());

    // A source directory that *equals* `dist` is swallowed whole.
    let cfg = parse("paths { dist \"public\"; static \"public\" }");
    let (key, _) = cfg.paths.swallowed(root).expect("static swallowed");
    assert_eq!(key, "static");
}

/// The conventional layout, and a `dist` outside the project, both stand: the
/// guard refuses containment, not any particular location.
#[test]
fn a_dist_beside_the_sources_is_accepted() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path();

    assert_eq!(Config::default().paths.swallowed(root), None);
    assert_eq!(
        parse("paths { dist \"../site\" }").paths.swallowed(root),
        None
    );
    assert_eq!(
        parse("paths { dist \"public/dist\"; assets \"src/assets\" }")
            .paths
            .swallowed(root),
        None
    );
}

/// `index` names a stem, matched against `Stem::slug`, which never carries an
/// extension. The docs shipped `index "index.typ"`, which matches no page: every
/// bundle keeps its filename slug, `content/index.typ` publishes to `/index/`,
/// and the build reports success with nothing at `/`.
#[test]
fn index_rejects_a_filename_and_names_the_stem() {
    let err = Config::parse("content { index \"index.typ\" }").expect_err("should refuse");
    let BaudelaireErrorKind::Config(config) = &err else {
        panic!("expected a config diagnostic, got: {err:?}");
    };
    assert_eq!(
        config.code().map(|c| c.to_string()).as_deref(),
        Some("baudelaire::config::index_extension")
    );
    // The help has to carry the correction, not just the complaint.
    let help = config.help().expect("a help").to_string();
    assert!(help.contains("index"), "help should name the stem: {help}");

    // The stem itself is what the key takes...
    assert_eq!(
        parse("content { index \"index\" }")
            .content
            .index
            .as_deref(),
        Some("index")
    );
    // ...and the empty string stays the documented way to turn bundles off.
    assert_eq!(parse("content { index \"\" }").content.index, None);
    // A stem that merely contains a dot is not a filename, and is left alone.
    assert_eq!(
        parse("content { index \"_index\" }")
            .content
            .index
            .as_deref(),
        Some("_index")
    );
}

#[test]
fn schema_reads_the_type_as_a_positional_or_an_attribute() {
    let cfg = parse(
        r#"
        content {
            collections {
                blog {
                    schema {
                        title
                        tags "list"
                        hero type="str" optional=#true
                    }
                }
            }
        }
    "#,
    );
    let schema = cfg.schema("blog");
    assert_eq!(schema.len(), 3);
    // A bare field constrains presence and nothing else, and is still required.
    assert_eq!(schema[0].0, "title");
    assert_eq!(schema[0].1.ty, FieldType::Any);
    assert!(!schema[0].1.optional);
    assert_eq!(schema[1].1.ty, FieldType::List(Box::new(FieldType::Str)));
    assert_eq!(schema[2].1.ty, FieldType::Str);
    assert!(schema[2].1.optional);
    // A collection with no schema, and one with no config at all, constrain nothing.
    assert!(cfg.schema("notes").is_empty());
}

/// A field's type is an expression, and a block declares the fields of the
/// dictionary it ends in, however many lists wrap that dictionary.
#[test]
fn schema_reads_nested_types_and_the_fields_of_a_dict() {
    let cfg = parse(
        r#"
        content {
            collections {
                blog {
                    schema {
                        widths "list<int>"
                        author "dict" {
                            name "str"
                            email "str" optional=#true
                        }
                        authors "list<dict>" {
                            name "str"
                        }
                    }
                }
            }
        }
    "#,
    );
    let schema = cfg.schema("blog");
    assert_eq!(
        schema[0].1.ty,
        FieldType::List(Box::new(FieldType::Int)),
        "a list names what it holds"
    );

    let FieldType::Dict(fields) = &schema[1].1.ty else {
        panic!("expected a dict, got {:?}", schema[1].1.ty);
    };
    assert_eq!(fields.len(), 2);
    assert_eq!(fields[0].0, "name");
    assert_eq!(fields[0].1.ty, FieldType::Str);
    assert!(!fields[0].1.optional);
    assert!(
        fields[1].1.optional,
        "a nested field is optional on its own"
    );

    // The block attaches to the dictionary inside the list, not to the list.
    let FieldType::List(inner) = &schema[2].1.ty else {
        panic!("expected a list, got {:?}", schema[2].1.ty);
    };
    let FieldType::Dict(fields) = inner.as_ref() else {
        panic!("expected a dict element, got {inner:?}");
    };
    assert_eq!(fields.len(), 1);
    assert_eq!(fields[0].0, "name");
}

/// The type language's two failures, each at the line that wrote it.
#[test]
fn schema_refuses_a_broken_type_and_a_block_with_nowhere_to_go() {
    let code = |kdl: &str| {
        let err = Config::parse(kdl).expect_err("should refuse");
        let BaudelaireErrorKind::Config(config) = &err else {
            panic!("expected a config diagnostic, got: {err:?}");
        };
        config.code().map(|c| c.to_string()).expect("a code")
    };
    assert_eq!(
        code("content { collections { blog { schema { hero \"list<int\" } } } }"),
        "baudelaire::config::type_expr"
    );
    assert_eq!(
        code("content { collections { blog { schema { hero \"list<nope>\" } } } }"),
        "baudelaire::config::unknown_value"
    );
    // A block declares what a dictionary holds, so a type with none has no use
    // for one: the mistake fails here rather than being silently dropped.
    assert_eq!(
        code("content { collections { blog { schema { hero \"list<int>\" { name \"str\" } } } } }"),
        "baudelaire::config::field_not_dict"
    );
}

/// A built-in key's type is fixed by the frontmatter reader, so a schema
/// declaring another one could never be satisfied: it fails at the config line
/// rather than on every page of the collection.
#[test]
fn schema_refuses_a_type_a_builtin_key_cannot_hold() {
    let err = Config::parse("content { collections { blog { schema { title \"int\" } } } }")
        .expect_err("should refuse");
    let BaudelaireErrorKind::Config(config) = &err else {
        panic!("expected a config diagnostic, got: {err:?}");
    };
    assert_eq!(
        config.code().map(|c| c.to_string()).as_deref(),
        Some("baudelaire::config::field_conflict")
    );
    // Declaring the type it does hold, or none at all, is how you require it.
    assert_eq!(
        parse("content { collections { blog { schema { title \"str\" } } } }").schema("blog")[0]
            .1
            .ty,
        FieldType::Str
    );
    assert_eq!(
        parse("content { collections { blog { schema { date } } } }").schema("blog")[0]
            .1
            .ty,
        FieldType::Any
    );
}

/// The two places a layout can be named, nearer first, and the root pages that
/// reach the second through the collection they are discovered into: a theme
/// binds `_root` and a page that names nothing still renders through it.
#[test]
fn a_template_binding_resolves_nearest_first() {
    let config = parse(
        "content { collections { _root { template \"site.typ\" }; posts { template \"post.typ\" } } }",
    );
    assert_eq!(
        config
            .template_for("posts", Some("own.typ".into()))
            .as_deref(),
        Some("own.typ")
    );
    assert_eq!(
        config.template_for("posts", None).as_deref(),
        Some("post.typ")
    );
    assert_eq!(
        config.template_for(crate::content::ROOT, None).as_deref(),
        Some("site.typ")
    );
    // A collection nothing configures binds nothing: the page renders unwrapped.
    assert_eq!(config.template_for("notes", None), None);
}

/// The whole point of the shorthand: the one-boolean case, which is what a
/// `dev` profile writes, does not have to open a block to say it, and every
/// sibling key of the section it stands in for is left alone.
#[test]
fn drafts_takes_a_bare_flag_for_its_build_key() {
    assert!(!parse("").content.drafts.build, "off by default");
    assert!(parse("content { drafts #true }").content.drafts.build);
    assert!(parse("content { drafts }").content.drafts.build, "bare");
    assert!(!parse("content { drafts #false }").content.drafts.build);
    let cfg = parse("content { drafts #true }");
    assert_eq!(cfg.content.drafts.suffix, ".draft", "sibling untouched");
}

/// The long spelling still reads, and an argument may carry a block: the
/// shorthand is an extra spelling of one key, not a replacement for the block.
#[test]
fn drafts_still_takes_its_block_with_or_without_the_flag() {
    let cfg = parse("content { drafts { build #true; suffix \".wip\" } }");
    assert!(cfg.content.drafts.build);
    assert_eq!(cfg.content.drafts.suffix, ".wip");
    let cfg = parse("content { drafts #true { suffix \".wip\" } }");
    assert!(cfg.content.drafts.build);
    assert_eq!(cfg.content.drafts.suffix, ".wip");
}

/// The argument reaches the `build` handler untouched, so the shorthand cannot
/// accept a value `drafts { build .. }` would refuse.
#[test]
fn a_non_boolean_draft_shorthand_is_a_type_error() {
    let err = Config::parse("content { drafts \"yes\" }")
        .expect_err("string is not a boolean")
        .to_string();
    assert!(err.contains("expected boolean"), "{err}");
}

/// The key was `draft` through 0.0.11, and the rename is a hard error rather
/// than a silent no-op: an ignored `draft { build #true }` is a production
/// build that quietly drops every draft page.
#[test]
fn the_old_draft_spelling_is_refused_with_a_suggestion() {
    let err = Config::parse("content { draft { build #true } }").expect_err("renamed key");
    let rendered = format!("{:?}", miette::Report::from(err));
    assert!(rendered.contains("did you mean `drafts`?"), "{rendered}");
}
