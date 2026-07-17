use crate::config::{Config, ImageFormat, PngStrip, SortKey};

fn parse(text: &str) -> Config {
    Config::parse(text).expect("should parse")
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
    assert_eq!(cfg.urls, crate::config::UrlStyle::Clean);
    assert!(cfg.clean);
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
    assert_eq!(parse("").index.as_deref(), Some("index"));
    assert_eq!(
        parse("paths {\n  index \"_index\"\n}").index.as_deref(),
        Some("_index")
    );
    // an empty basename disables bundle slugs
    assert_eq!(parse("paths {\n  index \"\"\n}").index, None);
}

#[test]
fn scalars() {
    let cfg = parse(
        r#"
        site "Baudelaire"
        url "https://example.net"
        lang "fr"
        author "Claude"
        future #true
    "#,
    );
    assert_eq!(cfg.site.as_deref(), Some("Baudelaire"));
    assert_eq!(cfg.url.as_deref(), Some("https://example.net"));
    assert_eq!(cfg.lang, "fr");
    assert_eq!(cfg.author.as_deref(), Some("Claude"));
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
          features "+html" "pdf"
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
fn err_feature_removal_rejected() {
    let err = Config::parse("typst {\n  features \"-pdf\"\n}\n").unwrap_err();
    assert!(
        err.to_string()
            .contains("removing feature `pdf` is not supported"),
        "{err}"
    );
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
    // extension matching is lenient and case-insensitive
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
    // an unlisted format stays off
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
          static "public-files"
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
    assert_eq!(cfg.r#static.to_str(), Some("public-files"));
    assert!(!cfg.sitemap);
    assert!(cfg.client.is_empty());
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
    let rendered = format!("{:?}", miette::Report::from(err));
    // an unknown enum *value* reads as "unknown value", not "unknown key"
    assert!(rendered.contains("unknown value `wat`"), "{rendered}");
    assert!(rendered.contains("order, date, title"), "{rendered}");
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
    // A bare flag node inside a block enables; the top-level default is on too.
    assert!(parse("").clean);
    assert!(parse("output {\n  clean\n}").clean);
    assert!(!parse("output {\n  clean #false\n}").clean);
}

#[test]
fn err_boolean_type() {
    let err = Config::parse("output {\n  clean \"yes\"\n}").unwrap_err();
    assert!(
        err.to_string().contains("expected boolean, got string"),
        "{err}"
    );
}

#[test]
fn url_style_parsed_from_output_block() {
    use crate::config::UrlStyle;
    assert_eq!(parse("output {\n  urls \"clean\"\n}").urls, UrlStyle::Clean);
    assert_eq!(parse("output {\n  urls \"flat\"\n}").urls, UrlStyle::Flat);
}

#[test]
fn err_unknown_url_style() {
    let err = Config::parse("output {\n  urls \"pretty\"\n}").unwrap_err();
    let rendered = format!("{:?}", miette::Report::from(err));
    // The value table drives the "did you mean" hint, listing the valid styles.
    assert!(rendered.contains("unknown value `pretty`"), "{rendered}");
    assert!(rendered.contains("clean, flat"), "{rendered}");
}

#[test]
fn err_boolean_attr_type() {
    let err = Config::parse("collections {\n  posts reverse=\"yes\"\n}\n").unwrap_err();
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
    let err = Config::parse("output {\n  feed {\n    limit -1\n  }\n}\n").unwrap_err();
    assert!(
        err.to_string()
            .contains("`limit` must not be negative, got -1"),
        "{err}"
    );
    let err = Config::parse("output {\n  search {\n    minimum -2\n  }\n}\n").unwrap_err();
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
            "collections {\n  posts paginate=0\n}\n",
            "paginate must be at least 1, got 0",
        ),
        (
            "collections {\n  posts paginate=-3\n}\n",
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
        Config::parse("output {\n  feed {\n    formats \"rss\" \"rss\"\n  }\n}\n").unwrap_err();
    assert!(
        err.to_string().contains("duplicate `rss` in `formats`"),
        "{err}"
    );
}

#[test]
fn err_duplicate_collection() {
    let err = Config::parse("collections {\n  posts\n  posts sort=\"date\"\n}\n").unwrap_err();
    assert!(
        err.to_string().contains("duplicate collection `posts`"),
        "{err}"
    );
}

#[test]
fn err_duplicate_taxonomy() {
    let err = Config::parse("taxonomies {\n  tags\n  tags index=#true\n}\n").unwrap_err();
    assert!(
        err.to_string().contains("duplicate taxonomy `tags`"),
        "{err}"
    );
}

#[test]
fn err_duplicate_profile() {
    let err = Config::parse("profiles {\n  dev { future #true }\n  dev { future #false }\n}\n")
        .unwrap_err();
    assert!(err.to_string().contains("duplicate profile `dev`"), "{err}");
}

#[test]
fn err_unknown_permalink_placeholder_is_spanned() {
    let text = "collections {\n  posts permalink=\"/{bogus}/\"\n}\n";
    let err = Config::parse(text).unwrap_err();
    let rendered = format!("{:?}", miette::Report::from(err));
    assert!(
        rendered.contains("unknown permalink placeholder `bogus`"),
        "{rendered}"
    );
    assert!(rendered.contains("valid placeholders"), "{rendered}");
    // the label excerpts config.kdl: the error is spanned at parse time
    assert!(rendered.contains("permalink="), "{rendered}");
}

#[test]
fn err_unterminated_permalink_placeholder() {
    let err = Config::parse("collections {\n  posts permalink=\"/{slug\"\n}\n").unwrap_err();
    assert!(err.to_string().contains("unterminated `{slug`"), "{err}");
}

#[test]
fn err_permalink_parent_dir_segment() {
    let err = Config::parse("collections {\n  posts permalink=\"/../{slug}/\"\n}\n").unwrap_err();
    assert!(err.to_string().contains("must not contain `..`"), "{err}");
}

#[test]
fn err_unexpected_positional_argument() {
    // taxonomies take no positional arguments
    let err = Config::parse("taxonomies {\n  tags \"extra\"\n}\n").unwrap_err();
    assert!(err.to_string().contains("unexpected argument"), "{err}");
    // collections consume exactly one (the glob); a second is discarded today
    let err = Config::parse("collections {\n  posts \"posts/*.typ\" \"extra\"\n}\n").unwrap_err();
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
    let dist = cfg.dist.clone();
    for url in ["/../../etc/passwd/", "/posts/../../secret/"] {
        let dest = cfg.destination(url);
        assert!(dest.starts_with(&dist), "{url} -> {}", dest.display());
        assert!(
            !dest.components().any(|c| c.as_os_str() == ".."),
            "{url} -> {}",
            dest.display()
        );
    }
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
    assert_eq!(s3.region, "auto");
    assert_eq!(s3.prefix, "");
    assert!(s3.delete, "delete defaults on");
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
fn images_extract_defaults_off_and_parses() {
    assert!(!parse("").images.extract);
    let cfg = parse("output {\n  images {\n    extract #true\n  }\n}\n");
    assert!(cfg.images.extract);
}

#[test]
fn externalize_gate_yields_to_embed() {
    // `extract` alone externalizes; `html.embed` (which re-inlines assets)
    // overrides it so the two never fight.
    let extract = parse("output {\n  images { extract #true }\n}\n");
    assert!(extract.images.externalize(&extract.html));
    let both = parse("output {\n  html { embed #true }\n  images { extract #true }\n}\n");
    assert!(!both.images.externalize(&both.html));
}

#[test]
fn responsive_block_enables_with_default_widths() {
    assert!(!parse("").images.responsive.enabled);
    let cfg = parse("output {\n  images {\n    responsive { }\n  }\n}\n");
    assert!(cfg.images.responsive.enabled);
    // silent block keeps the built-in breakpoints and quality.
    assert_eq!(cfg.images.responsive.widths, vec![480, 960, 1440]);
    assert_eq!(cfg.images.responsive.quality, 80);
    assert!(cfg.images.responsive.sizes.is_none());
}

#[test]
fn responsive_widths_and_sizes_override() {
    let cfg = parse(
        "output {\n  images {\n    responsive {\n      widths 320 640\n      quality 70\n      sizes \"50vw\"\n    }\n  }\n}\n",
    );
    assert_eq!(cfg.images.responsive.widths, vec![320, 640]);
    assert_eq!(cfg.images.responsive.quality, 70);
    assert_eq!(cfg.images.responsive.sizes.as_deref(), Some("50vw"));
}

#[test]
fn responsive_rejects_a_zero_width() {
    // widths are 1..=16384; a 0 (or negative) is a hard error, not a silent drop.
    assert!(Config::parse("output {\n  images {\n    responsive { widths 0 }\n  }\n}\n").is_err());
}
