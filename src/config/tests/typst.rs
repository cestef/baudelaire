//! `typst { }`: features, inputs, fonts, and the package registry.

use std::path::PathBuf;

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

/// A site that names no directory still sees the machine's fonts, which is the
/// only behaviour there has ever been.
#[test]
fn fonts_default_to_the_machines_own() {
    let cfg = Config::default();
    assert!(cfg.typst.fonts.paths.is_empty());
    assert!(cfg.typst.fonts.system);
}

#[test]
fn font_directories_and_the_system_switch_are_read() {
    let cfg = parse(
        "typst {\n  fonts {\n    paths \"assets/fonts\" \"vendor/fonts\"\n    system #false\n  }\n}\n",
    );
    assert_eq!(
        cfg.typst.fonts.paths,
        vec![PathBuf::from("assets/fonts"), PathBuf::from("vendor/fonts")]
    );
    assert!(!cfg.typst.fonts.system);
}

/// A directory that is not there yields no faces and no error at compile time,
/// so the typo has to be caught while it can still be told from a directory that
/// simply holds no fonts.
#[test]
fn a_font_directory_that_is_not_there_is_named() {
    let root = std::path::Path::new("/nowhere-at-all");
    let cfg = parse("typst {\n  fonts {\n    paths \"fonts\"\n  }\n}\n");
    assert_eq!(
        cfg.typst.fonts.missing(root),
        Some(std::path::Path::new("fonts"))
    );
    // The project root itself is a directory, so naming it is not a mistake.
    let cfg = parse("typst {\n  fonts {\n    paths \".\"\n  }\n}\n");
    assert_eq!(cfg.typst.fonts.missing(std::path::Path::new(".")), None);
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
