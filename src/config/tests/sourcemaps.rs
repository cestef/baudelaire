//! `assets { sourcemap }`: a posture per kind of asset, and how the line and
//! the block compose into one.

use super::{err, parse};
use crate::config::SourceMaps;

/// The line stands for every kind, and a block narrows from it.
#[test]
fn a_sourcemap_posture_on_the_line_sets_every_kind() {
    let cfg = parse("assets {\n  sourcemap \"external\"\n}");
    assert_eq!(cfg.assets.sourcemap.scripts, SourceMaps::External);
    assert_eq!(cfg.assets.sourcemap.styles, SourceMaps::External);

    let cfg = parse("assets {\n  sourcemap \"external\" {\n    styles \"off\"\n  }\n}");
    assert_eq!(cfg.assets.sourcemap.scripts, SourceMaps::External);
    assert_eq!(cfg.assets.sourcemap.styles, SourceMaps::Off);
}

#[test]
fn each_kind_takes_its_own_posture() {
    let cfg =
        parse("assets {\n  sourcemap {\n    scripts \"hidden\"\n    styles \"inline\"\n  }\n}");
    assert_eq!(cfg.assets.sourcemap.scripts, SourceMaps::Hidden);
    assert_eq!(cfg.assets.sourcemap.styles, SourceMaps::Inline);
}

#[test]
fn no_kind_is_mapped_by_default() {
    let cfg = parse("site \"T\"");
    assert!(!cfg.assets.sourcemap.scripts.wanted());
    assert!(!cfg.assets.sourcemap.styles.wanted());
}

/// A profile fills in place, so narrowing one kind must leave the base's choice
/// for the other alone, which is why the line's value is required rather than
/// defaulted.
#[test]
fn a_profile_narrows_one_kind_and_leaves_the_other() {
    let cfg = parse(
        "assets {\n  sourcemap \"hidden\"\n}\nprofiles {\n  dev {\n    assets {\n      sourcemap {\n        styles \"inline\"\n      }\n    }\n  }\n}",
    )
    .with_profile("dev")
    .expect("the profile applies");
    assert_eq!(cfg.assets.sourcemap.scripts, SourceMaps::Hidden);
    assert_eq!(cfg.assets.sourcemap.styles, SourceMaps::Inline);
}

#[test]
fn err_an_unknown_posture_lists_the_ones_there_are() {
    let rendered = err("assets {\n  sourcemap \"maybe\"\n}");
    assert!(rendered.contains("external"), "{rendered}");
    assert!(rendered.contains("hidden"), "{rendered}");
}

/// Guessing a posture for a bare node is how a profile silently overrides its
/// base.
#[test]
fn err_a_bare_sourcemap_node_asks_for_nothing() {
    let rendered = err("assets {\n  sourcemap\n}");
    assert!(!rendered.is_empty(), "{rendered}");
}
