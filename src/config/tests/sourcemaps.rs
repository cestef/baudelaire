//! `assets { sourcemap }`: a posture per kind of asset, and how the line and the
//! block compose into one.
//!
//! The value is what carries the meaning here, not a flag: `off`, `inline`,
//! `external` and `hidden` are four different answers to who is meant to read
//! the map, and they are exclusive. Written as booleans they admitted states
//! that say nothing (a map both inline and unreferenced), so the tests below are
//! as much about what cannot be spelled as about what can.

use super::{err, parse};
use crate::config::SourceMaps;

/// The line stands for every kind, and a block narrows from it, so the two
/// spellings compose into one sentence rather than contradicting each other.
#[test]
fn a_sourcemap_posture_on_the_line_sets_every_kind() {
    let cfg = parse("assets {\n  sourcemap \"external\"\n}");
    assert_eq!(cfg.assets.sourcemap.scripts, SourceMaps::External);
    assert_eq!(cfg.assets.sourcemap.styles, SourceMaps::External);

    let cfg = parse("assets {\n  sourcemap \"external\" {\n    styles \"off\"\n  }\n}");
    assert_eq!(cfg.assets.sourcemap.scripts, SourceMaps::External);
    assert_eq!(cfg.assets.sourcemap.styles, SourceMaps::Off);
}

/// Each kind can be named on its own, with no line value at all.
#[test]
fn each_kind_takes_its_own_posture() {
    let cfg =
        parse("assets {\n  sourcemap {\n    scripts \"hidden\"\n    styles \"inline\"\n  }\n}");
    assert_eq!(cfg.assets.sourcemap.scripts, SourceMaps::Hidden);
    assert_eq!(cfg.assets.sourcemap.styles, SourceMaps::Inline);
}

/// Nothing is mapped unless the site says so, which is what keeps a site's
/// sources unpublished by default.
#[test]
fn no_kind_is_mapped_by_default() {
    let cfg = parse("site \"T\"");
    assert!(!cfg.assets.sourcemap.scripts.wanted());
    assert!(!cfg.assets.sourcemap.styles.wanted());
}

/// A profile fills in place, so narrowing one kind must leave the base's choice
/// for the other alone. This is the whole reason the line's value is required
/// rather than defaulted: a default would be re-applied here and silently undo
/// what the base config chose.
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

/// A name outside the set names the ones that exist.
#[test]
fn err_an_unknown_posture_lists_the_ones_there_are() {
    let rendered = err("assets {\n  sourcemap \"maybe\"\n}");
    assert!(rendered.contains("external"), "{rendered}");
    assert!(rendered.contains("hidden"), "{rendered}");
}

/// Neither a value nor a block asks for nothing at all, and guessing is how a
/// profile silently overrides its base.
#[test]
fn err_a_bare_sourcemap_node_asks_for_nothing() {
    let rendered = err("assets {\n  sourcemap\n}");
    assert!(!rendered.is_empty(), "{rendered}");
}
