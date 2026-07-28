use crate::config::{Config, ImageFormat, PngStrip, SortKey};

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
            posts "posts/**/*.typ" sort="date" reverse=#true permalink="/posts/{slug}/"
            notes "notes/**/*.typ" sort="order"
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
    let err =
        Config::parse("content {\n  collections {\n    posts sort=\"wat\"\n  }\n}\n").unwrap_err();
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
    assert!(rendered.contains("clean, flat"), "{rendered}");
}

#[test]
fn err_boolean_attr_type() {
    let err = Config::parse("content {\n  collections {\n    posts reverse=\"yes\"\n  }\n}\n")
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
            "content {\n  collections {\n    posts paginate=0\n  }\n}\n",
            "paginate must be at least 1, got 0",
        ),
        (
            "content {\n  collections {\n    posts paginate=-3\n  }\n}\n",
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
    let err =
        Config::parse("content {\n  collections {\n    posts\n    posts sort=\"date\"\n  }\n}\n")
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
    let text = "content {\n  collections {\n    posts permalink=\"/{bogus}/\"\n  }\n}\n";
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
    let err = Config::parse("content {\n  collections {\n    posts permalink=\"/{slug\"\n  }\n}\n")
        .unwrap_err();
    assert!(err.to_string().contains("unterminated `{slug`"), "{err}");
}

#[test]
fn err_permalink_parent_dir_segment() {
    let err =
        Config::parse("content {\n  collections {\n    posts permalink=\"/../{slug}/\"\n  }\n}\n")
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
        let dest = cfg.destination(url);
        assert!(dest.starts_with(&dist), "{url} -> {}", dest.display());
        assert!(
            !dest.components().any(|c| c.as_os_str() == ".."),
            "{url} -> {}",
            dest.display()
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
