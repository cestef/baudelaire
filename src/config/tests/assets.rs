//! `assets { minify { } }` and `assets { targets { } }`.

use super::{code, err, parse};
use crate::config::{Config, Version};

#[test]
fn minify_keeps_the_flag_it_used_to_be() {
    let off = Config::default().assets.minify;
    assert!(!off.css() && !off.js());

    for text in ["assets {\n  minify\n}", "assets {\n  minify #true\n}"] {
        let on = parse(text).assets.minify;
        assert!(on.css() && on.js(), "{text}");
    }
    let back = parse("assets {\n  minify #false\n}").assets.minify;
    assert!(!back.css() && !back.js());
}

#[test]
fn minify_names_one_kind_without_losing_the_other() {
    let one = parse("assets {\n  minify {\n    js #false\n  }\n}")
        .assets
        .minify;
    assert!(one.css(), "the block's presence still turns css on");
    assert!(!one.js());

    let neither = parse("assets {\n  minify #false {\n    css #true\n  }\n}")
        .assets
        .minify;
    assert!(neither.css(), "the block reads behind the flag");
    assert!(!neither.js());
}

/// A profile naming one kind must not answer for the other, `Section::fill`
/// running a section's switch on every mention.
#[test]
fn a_profile_naming_one_kind_leaves_the_other_alone() {
    let config = parse(
        "assets {\n  minify {\n    js #false\n  }\n}\n\
         profiles {\n  dev {\n    assets {\n      minify {\n        css #true\n      }\n    }\n  }\n}",
    )
    .with_profile("dev")
    .expect("profile applies");
    assert!(config.assets.minify.css());
    assert!(
        !config.assets.minify.js(),
        "the profile said nothing about js"
    );
}

#[test]
fn targets_are_versions_packed_one_byte_per_component() {
    let cfg = parse(
        "assets {\n  targets {\n    chrome \"80\"\n    safari \"13.1\"\n    ios \"15.4.1\"\n  }\n}",
    );
    let targets = &cfg.assets.targets;
    assert!(targets.any());
    assert_eq!(targets.chrome, Some(Version(80 << 16)));
    assert_eq!(targets.safari, Some(Version((13 << 16) | (1 << 8))));
    assert_eq!(targets.ios, Some(Version((15 << 16) | (4 << 8) | 1)));
    assert_eq!(targets.firefox, None);
    assert!(!Config::default().assets.targets.any());
}

/// A version is a string on purpose: `15.10` written as a KDL number is the
/// float `15.1`, which is a different browser.
#[test]
fn a_version_that_is_not_one_is_refused() {
    for text in [
        "assets {\n  targets {\n    chrome \"latest\"\n  }\n}",
        "assets {\n  targets {\n    chrome \"1.2.3.4\"\n  }\n}",
        "assets {\n  targets {\n    chrome \"300\"\n  }\n}",
    ] {
        assert_eq!(code(text), "baudelaire::config::bad_version", "{text}");
    }
    for text in [
        "assets {\n  targets {\n    chrome 80\n  }\n}",
        "assets {\n  targets {\n    safari 15.4\n  }\n}",
    ] {
        assert_eq!(code(text), "baudelaire::config::bad_version", "{text}");
    }
    assert!(
        err("assets {\n  targets {\n    safari 15.4\n  }\n}").contains("15.4"),
        "the message quotes the value as written"
    );
}

#[test]
fn an_unknown_browser_is_refused() {
    assert_eq!(
        code("assets {\n  targets {\n    crhome \"80\"\n  }\n}"),
        "baudelaire::config::unknown_key"
    );
}
