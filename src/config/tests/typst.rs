//! `typst { }`: features, inputs, and the package registry.

use super::parse;
use crate::config::Config;
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
