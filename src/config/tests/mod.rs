mod arguments;
mod assets;
mod content;
mod images;
mod languages;
mod links;
mod remote;
mod schema;
mod sourcemaps;
mod switches;
mod typst;
mod urls;
mod values;

use crate::config::Config;

pub(super) fn parse(text: &str) -> Config {
    Config::parse(text).expect("should parse")
}

/// The rendered diagnostic of a config that must not parse: message, help and
/// snippet, as a reader actually sees them.
pub(super) fn err(text: &str) -> String {
    let error = Config::parse(text).expect_err("should be refused");
    format!("{:?}", miette::Report::from(error))
}

/// The diagnostic code a config that must not parse reports. Beside [`err`]
/// because the two are different contracts: the prose is for a reader, the code
/// is what a caller and a scenario key on, and only the second is stable.
pub(super) fn code(text: &str) -> String {
    let error = Config::parse(text).expect_err("should be refused");
    miette::Diagnostic::code(&error).map_or_else(String::new, |code| code.to_string())
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
    assert!(cfg.prune.enabled);
    assert!(cfg.html.pretty);
    assert_eq!(cfg.serve.port, 1821);
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
        content { future #true }
    "#,
    );
    assert_eq!(cfg.site.as_deref(), Some("Baudelaire"));
    assert_eq!(cfg.url.as_deref(), Some("https://example.net"));
    assert_eq!(cfg.lang, "fr");
    assert_eq!(cfg.author.as_deref(), Some("Claude"));
    assert!(cfg.content.future);
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
    assert!(parse("").prune.enabled);
    assert!(parse("prune").prune.enabled);
    assert!(!parse("prune #false").prune.enabled);
}

/// `prune` grew a block without losing the flag it has always been: the keep
/// list reads on its own, and the flag still stands in front of it.
#[test]
fn prune_keeps_what_it_is_told_to() {
    assert!(parse("prune").prune.keep.is_empty());

    let cfg = parse("prune {\n  keep \"themes/**\" \"v*/**\"\n}\n");
    assert!(cfg.prune.enabled, "a block alone still enables");
    assert_eq!(cfg.prune.keep, ["themes/**", "v*/**"]);

    let off = parse("prune #false {\n  keep \"themes/**\"\n}\n");
    assert!(!off.prune.enabled);
    assert_eq!(off.prune.keep, ["themes/**"]);
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
fn err_duplicate_format() {
    let err =
        Config::parse("generate {\n  feed {\n    formats \"rss\" \"rss\"\n  }\n}\n").unwrap_err();
    assert!(
        err.to_string().contains("duplicate `rss` in `formats`"),
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

/// `html { anchors }` was a flag and stays one, with a block behind it: the id
/// half has always been on, and the link half is opt-in because it is markup the
/// site did not write.
#[test]
fn anchors_keep_the_flag_and_gain_a_block() {
    use crate::config::Place;

    let default = Config::default().html.anchors;
    assert!(
        default.enabled,
        "ids are derived unless a site says otherwise"
    );
    assert!(default.levels.is_empty(), "every level, until narrowed");
    assert_eq!(default.link, None, "no link until asked for");

    assert!(!parse("html {\n  anchors #false\n}").html.anchors.enabled);
    assert!(parse("html {\n  anchors\n}").html.anchors.enabled);

    let cfg =
        parse("html {\n  anchors {\n    levels 2 3\n    link \"#\"\n    place \"before\"\n  }\n}");
    let anchors = &cfg.html.anchors;
    assert!(anchors.enabled, "the block's presence is still the switch");
    assert_eq!(anchors.levels, vec![2, 3]);
    assert_eq!(anchors.link.as_deref(), Some("#"));
    assert_eq!(anchors.place, Place::Before);
    // An empty list is every level; a narrowed one covers what it names.
    assert!(anchors.covers(2) && anchors.covers(3));
    assert!(!anchors.covers(1) && !anchors.covers(4));
    assert!(default.covers(1) && default.covers(6));
}

/// There are six heading levels, so `levels 0` and `levels 7` are typos that
/// would narrow the set to nothing at all.
#[test]
fn err_a_heading_level_that_is_not_one_is_refused() {
    for text in [
        "html {\n  anchors {\n    levels 0\n  }\n}",
        "html {\n  anchors {\n    levels 2 7\n  }\n}",
    ] {
        assert_eq!(code(text), "baudelaire::config::out_of_range", "{text}");
    }
}

/// A lint rule takes the boolean it always did, or a severity of its own, and
/// `strict` is the default the unnamed ones follow.
#[test]
fn a_lint_rule_names_its_own_severity_or_follows_strict() {
    use crate::config::{Ruled, Severity};

    let lenient = parse("lint { }");
    assert_eq!(lenient.lint.severity(Ruled::Alt), Severity::Warn);

    let strict = parse("lint {\n  strict\n}");
    assert_eq!(strict.lint.severity(Ruled::Alt), Severity::Error);
    assert_eq!(strict.lint.severity(Ruled::Headings), Severity::Error);

    // Named, and so kept: `strict` is a default, not an override.
    let mixed = parse("lint {\n  strict\n  headings \"warn\"\n  ids \"off\"\n}");
    assert_eq!(mixed.lint.severity(Ruled::Alt), Severity::Error);
    assert_eq!(mixed.lint.severity(Ruled::Headings), Severity::Warn);
    assert_eq!(mixed.lint.severity(Ruled::Ids), Severity::Off);
    assert!(!mixed.lint.ids.on());
    assert!(mixed.lint.headings.on(), "a warning rule still runs");

    // The flag spelling every site already wrote.
    let flags = parse("lint {\n  strict\n  aria #false\n  alt #true\n}");
    assert_eq!(flags.lint.severity(Ruled::Aria), Severity::Off);
    assert_eq!(
        flags.lint.severity(Ruled::Alt),
        Severity::Error,
        "`#true` is on, and `strict` says how loud"
    );
}

#[test]
fn err_a_severity_that_is_not_one_is_refused() {
    assert_eq!(
        code("lint {\n  alt \"loud\"\n}"),
        "baudelaire::config::unknown_value"
    );
}

/// A budget fails by default and can be turned down to a report, which a site
/// adopting one on existing pages needs.
#[test]
fn a_budget_can_report_instead_of_failing() {
    assert!(parse("lint { }").lint.budget.strict);
    assert!(
        !parse("lint {\n  budget {\n    strict #false\n  }\n}")
            .lint
            .budget
            .strict
    );
}
