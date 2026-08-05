//! What a config node's own line may carry: the arguments a key reads, and the
//! ones nothing reads.
//!
//! Every refusal here used to be silent. A node-keyed rule dispatched on the
//! name alone, so a value or a `key=value` written beside it parsed green and
//! configured nothing -- or, where the section was turned on by its own
//! presence, the exact opposite of what it said.

use super::parse;
use crate::config::Config;

/// The rendered diagnostic of a config that must not parse.
fn err(text: &str) -> String {
    let err = Config::parse(text).expect_err("should be refused");
    format!("{:?}", miette::Report::from(err))
}

/// A section is configured from its block, so a value on its line is read by
/// nobody. Each of these parsed, and each turned the section *on*.
#[test]
fn err_a_value_on_a_section_line_is_refused() {
    for (config, node) in [
        ("lint #false", "lint"),
        ("generate {\n  cards #false\n}", "cards"),
        ("security {\n  csp #false\n}", "csp"),
        ("generate {\n  robots \"junk\"\n}", "robots"),
        (
            "content {\n  collections \"junk\" {\n    posts\n  }\n}",
            "collections",
        ),
        (
            "content {\n  taxonomies \"junk\" {\n    tags\n  }\n}",
            "taxonomies",
        ),
        ("profiles \"junk\" {\n  dev { prune #true }\n}", "profiles"),
    ] {
        let rendered = err(config);
        assert!(
            rendered.contains("unexpected argument"),
            "{config}: {rendered}"
        );
        assert!(
            rendered.contains(&format!("`{node}` is configured from its block")),
            "{config}: {rendered}"
        );
    }
}

/// A scalar key reads one value, and the second was dropped in silence.
#[test]
fn err_a_second_value_on_a_scalar_key_is_refused() {
    for (config, node) in [
        ("serve {\n  port 1 2\n}", "port"),
        ("site \"a\" \"b\"", "site"),
        ("prune #false #true", "prune"),
        (
            "content {\n  collections {\n    posts \"a/*.typ\" \"b/*.typ\"\n  }\n}",
            "posts",
        ),
    ] {
        let rendered = err(config);
        assert!(
            rendered.contains("unexpected argument"),
            "{config}: {rendered}"
        );
        assert!(
            rendered.contains(&format!("`{node}` reads a single value")),
            "{config}: {rendered}"
        );
    }
}

/// A node-keyed scope spells its keys as child nodes, so a `key=value` beside
/// one is read by nobody -- and the key is usually a real one, written in the
/// single spelling its scope does not take. The help writes the line the author
/// meant, values and all.
#[test]
fn err_a_key_value_on_a_node_keyed_line_shows_the_block_spelling() {
    for (config, message, example) in [
        (
            "content {\n  drafts suffix=\".x\"\n}",
            "unexpected attribute `suffix` on `drafts`",
            "drafts { suffix \".x\" }",
        ),
        (
            "content {\n  markdown eval=#false\n}",
            "unexpected attribute `eval` on `markdown`",
            "markdown { eval #false }",
        ),
        (
            "content {\n  collections {\n    posts sort=\"date\"\n  }\n}",
            "unexpected attribute `sort` on `posts`",
            "posts { sort \"date\" }",
        ),
        (
            "serve port=8080",
            "unexpected attribute `port` on `serve`",
            "serve { port 8080 }",
        ),
        // The value is quoted back as KDL, not printed: written bare, a value
        // with a space is a line the help offers and the parser refuses.
        (
            "content {\n  drafts suffix=\".x y\"\n}",
            "unexpected attribute `suffix` on `drafts`",
            "drafts { suffix \".x y\" }",
        ),
    ] {
        let rendered = err(config);
        assert!(rendered.contains(message), "{config}: {rendered}");
        assert!(rendered.contains(example), "{config}: {rendered}");
    }
}

/// Every shipped theme turns highlighting on and `highlight { enabled }` is not
/// a key, so until the flag was read there was no spelling at all that turned it
/// back off.
#[test]
fn highlight_reads_the_flag_that_turns_it_off() {
    assert!(!parse("").html.highlight.enabled, "off by default");
    assert!(
        parse("html {\n  highlight\n}").html.highlight.enabled,
        "bare turns it on"
    );
    assert!(
        !parse("html {\n  highlight #false\n}")
            .html
            .highlight
            .enabled
    );
    // The block still names the scopes, with the flag or without it.
    let cfg = parse("html {\n  highlight {\n    keyword \"#e5d004\"\n  }\n}");
    assert!(cfg.html.highlight.enabled);
    assert_eq!(cfg.html.highlight.class("#e5d004"), "sx-keyword");
    let cfg = parse("html {\n  highlight #false {\n    keyword \"#e5d004\"\n  }\n}");
    assert!(
        !cfg.html.highlight.enabled,
        "a theme's floor is overridable"
    );
    assert_eq!(cfg.html.highlight.class("#e5d004"), "sx-keyword");
}

/// The rule is arity, not prohibition: every shape a key legitimately takes has
/// to keep parsing, including the three that read a whole line of values and the
/// two scopes whose entries belong to somebody else's reader.
#[test]
fn the_shapes_a_key_does_take_still_parse() {
    let cfg = parse("generate {\n  feed {\n    formats \"rss\" \"atom\"\n  }\n}");
    assert_eq!(cfg.generate.feed.formats.len(), 2, "a list of names");

    let cfg = parse("html {\n  footnotes \"article\" \"main\"\n}");
    assert_eq!(cfg.html.footnotes.targets().len(), 2, "a list of words");

    let cfg =
        parse("assets {\n  images {\n    responsive {\n      widths 320 640 960\n    }\n  }\n}");
    assert_eq!(
        cfg.assets.images.responsive.widths.len(),
        3,
        "a list of numbers"
    );

    let cfg = parse("assets {\n  images {\n    optimize {\n      png level=6\n    }\n  }\n}");
    assert!(
        cfg.assets.images.optimize.png.is_some(),
        "an attribute line"
    );

    let cfg = parse("content {\n  taxonomies {\n    tags listing=#true\n  }\n}");
    assert_eq!(cfg.content.taxonomies.len(), 1, "attribute lines, named");

    let cfg = parse("content {\n  drafts #true {\n    suffix \".wip\"\n  }\n}");
    assert!(
        cfg.content.drafts.build,
        "a section shorthand and its block"
    );
    assert_eq!(cfg.content.drafts.suffix, ".wip");

    let cfg = parse(
        "content {\n  collections {\n    posts \"p/*.typ\" {\n      sort \"date\"\n    }\n  }\n}",
    );
    assert_eq!(
        cfg.content.collections[0].1.glob.as_deref(),
        Some("p/*.typ"),
        "a leading positional the caller reads"
    );
}
