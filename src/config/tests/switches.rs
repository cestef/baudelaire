//! Turning a section back off: an overlay has no spelling for deleting a node,
//! so a section whose presence is its switch also reads a boolean on its line.

use super::{err, parse};
use crate::config::Config;

/// How a parsed config answers whether one switchable section is on.
type Reads = fn(&Config) -> bool;

/// Every section whose presence is its switch: the line that names it, with `@`
/// standing where its own argument goes, and how a config answers whether it is
/// on.
const SWITCHES: &[(&str, Reads)] = &[
    ("lint @", |c| c.lint.enabled),
    ("caching @", |c| c.caching.enabled),
    ("security {\n  csp @\n}", |c| c.security.csp.enabled),
    ("generate {\n  robots @\n}", |c| c.generate.robots.enabled),
    ("generate {\n  llms @\n}", |c| c.generate.llms.enabled),
    ("generate {\n  manifest @\n}", |c| {
        c.generate.manifest.enabled
    }),
    ("generate {\n  cards @\n}", |c| c.generate.cards.enabled),
    ("generate {\n  pdf {\n    pages @\n  }\n}", |c| {
        c.generate.pdf.pages.enabled
    }),
    ("links {\n  external @\n}", |c| c.links.external.enabled),
    ("assets {\n  minify @\n}", |c| c.assets.minify.css()),
    ("navigation {\n  spa @\n}", |c| c.navigation.spa.enabled),
    ("navigation {\n  standalone @\n}", |c| {
        c.navigation.standalone.enabled
    }),
    ("navigation {\n  speculation @\n}", |c| {
        c.navigation.speculation.enabled
    }),
    ("assets {\n  images {\n    responsive @\n  }\n}", |c| {
        c.assets.images.responsive.enabled
    }),
    (
        "content {\n  collections {\n    posts {\n      paginate @\n    }\n  }\n}",
        |c| {
            c.content
                .collections
                .first()
                .is_some_and(|(_, posts)| posts.paginate.enabled)
        },
    ),
];

fn written(line: &str, argument: &str) -> String {
    line.replace('@', argument)
}

#[test]
fn presence_is_still_the_switch() {
    for &(line, on) in SWITCHES {
        assert!(!on(&parse("")), "{line}: on by default");
        assert!(on(&parse(&written(line, ""))), "{line}: bare");
        assert!(on(&parse(&written(line, "#true"))), "{line}: #true");
        assert!(on(&parse(&written(line, "{\n}"))), "{line}: an empty block");
    }
}

#[test]
fn a_flag_on_the_line_turns_a_section_off() {
    for &(line, on) in SWITCHES {
        assert!(!on(&parse(&written(line, "#false"))), "{line}");
    }
}

/// The flag stands in front of the block rather than replacing it, so a site
/// can keep its settings and still say no.
#[test]
fn a_block_still_reads_behind_the_flag() {
    let cfg = parse("lint #false {\n  strict #true\n}");
    assert!(!cfg.lint.enabled, "the flag wins");
    assert!(cfg.lint.strict, "and the block is still read");

    let cfg = parse("generate {\n  robots #false {\n    disallow \"/private/\"\n  }\n}");
    assert!(!cfg.generate.robots.enabled);
    assert_eq!(cfg.generate.robots.disallow, vec!["/private/".to_owned()]);
}

/// An overlay can only *add* nodes, so the flag is a profile's only way to undo
/// a section the base turned on.
#[test]
fn a_profile_takes_back_what_the_base_turned_on() {
    let cfg = parse(
        r"
        lint { strict #true }
        generate { cards { width 800 } }
        profiles {
          dev {
            lint #false
            generate { cards #false }
          }
        }
    ",
    );
    assert!(cfg.lint.enabled, "the base is untouched");
    let dev = cfg.with_profile("dev").expect("profile exists");
    assert!(!dev.lint.enabled);
    assert!(dev.lint.strict, "and its siblings are still inherited");
    assert!(!dev.generate.cards.enabled);
    assert_eq!(dev.generate.cards.width, 800);
}

/// `caching` is the one switch that fills a policy in when it is thrown, so it
/// is also the one that must fill nothing when it is not.
#[test]
fn caching_off_states_no_policy() {
    let cfg = parse("caching");
    assert!(cfg.caching.enabled);
    assert!(cfg.caching.immutable.contains("immutable"));

    let cfg = parse("caching #false");
    assert!(!cfg.caching.enabled);
    assert!(cfg.caching.immutable.is_empty(), "no policy invented");
    assert!(cfg.caching.default.is_empty());
    assert_eq!(cfg.caching.header("/a.css", "assets", true), None);
}

#[test]
fn err_a_switch_reads_a_boolean_and_nothing_else() {
    for config in [
        "lint \"junk\"",
        "generate {\n  robots \"junk\"\n}",
        "navigation {\n  spa 1\n}",
    ] {
        let rendered = err(config);
        assert!(
            rendered.contains("expected boolean"),
            "{config}: {rendered}"
        );
    }
    let rendered = err("lint #false #true");
    assert!(rendered.contains("unexpected argument"), "{rendered}");
    assert!(
        rendered.contains("`lint` reads a single value"),
        "{rendered}"
    );
}

/// A section with no switch still refuses a value, `generate { pdf }` included:
/// it groups the per-page block and has no flag of its own for one to set.
#[test]
fn err_a_section_without_a_switch_still_refuses_a_value() {
    for (config, node) in [
        ("paths #false", "paths"),
        ("serve #false", "serve"),
        ("generate {\n  pdf #false\n}", "pdf"),
    ] {
        let rendered = err(config);
        assert!(
            rendered.contains(&format!("`{node}` is configured from its block")),
            "{config}: {rendered}"
        );
    }
}

/// `markdown` reaches the flag through its own `enabled` key rather than
/// through a section switch, and both spellings agree.
#[test]
fn the_markdown_precedent_is_untouched() {
    assert!(parse("").content.markdown.enabled);
    assert!(
        !parse("content {\n  markdown #false\n}")
            .content
            .markdown
            .enabled
    );
    assert!(
        !parse("content {\n  markdown {\n    enabled #false\n  }\n}")
            .content
            .markdown
            .enabled
    );
    assert!(
        parse("content {\n  markdown\n}").content.markdown.present,
        "and presence is still recorded on every path"
    );
}
