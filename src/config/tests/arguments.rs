//! What a config node's own line may carry: the arguments a key reads, and the
//! ones nothing reads.

use super::{err, parse};

/// A section with no switch of its own is configured from its block, so a value
/// on its line is read by nobody; one that has a switch reads a boolean there
/// instead (`switches.rs`).
#[test]
fn err_a_value_on_a_section_line_is_refused() {
    for (config, node) in [
        ("paths \"junk\"", "paths"),
        ("artifacts {\n  pdf \"junk\"\n}", "pdf"),
        (
            "content {\n  collections \"junk\" {\n    posts\n  }\n}",
            "collections",
        ),
        (
            "content {\n  taxonomies \"junk\" {\n    tags\n  }\n}",
            "taxonomies",
        ),
        ("profiles \"junk\" {\n  dev { prune #true }\n}", "profiles"),
        // A free table reads its block and nothing on its own line.
        ("client \"junk\" {\n  env \"prod\"\n}", "client"),
        ("typst {\n  inputs \"junk\" { key \"v\" }\n}", "inputs"),
        (
            "paths {\n  sources \"junk\" { docs \"../docs\" }\n}",
            "sources",
        ),
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

/// A bundle with no block never reached the check that refuses what nothing
/// reads, so a typo on its line parsed and vanished.
#[test]
fn err_a_blockless_bundle_line_is_still_checked() {
    for config in [
        "artifacts {\n  bundles {\n    guide \"junk\"\n  }\n}",
        "artifacts {\n  bundles {\n    guide extra=#true\n  }\n}",
    ] {
        let rendered = err(config);
        assert!(rendered.contains("unexpected"), "{config}: {rendered}");
    }
}

/// A free table's children are the author's own keys, so nothing reads an
/// attribute written on one: a build that accepted it dropped the value while
/// the site believed it was in effect.
#[test]
fn err_an_attribute_inside_a_free_table_is_refused() {
    for config in [
        "client {\n  env \"prod\" extra=\"dropped\"\n}",
        "typst {\n  inputs {\n    key \"v\" also=\"dropped\"\n  }\n}",
        "headers {\n  rules {\n    \"/v*/*\" {\n      X-Robots-Tag \"noindex\" mode=\"dropped\"\n    }\n  }\n}",
    ] {
        let rendered = err(config);
        assert!(
            rendered.contains("unexpected") && rendered.contains("dropped"),
            "{config}: {rendered}"
        );
    }
}

#[test]
fn err_a_second_value_on_a_scalar_key_is_refused() {
    for (config, node) in [
        ("serve {\n  port 1 2\n}", "port"),
        ("site \"a\" \"b\"", "site"),
        ("prune #false #true", "prune"),
        ("links {\n  style \"clean\" \"flat\"\n}", "style"),
        (
            "content {\n  markdown {\n    html \"drop\" \"refuse\"\n  }\n}",
            "html",
        ),
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
/// one is read by nobody, and the help writes the line the author meant.
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

/// A list key has no block to move a `key=value` into, so it is refused as the
/// value it is not, rather than helped towards a line that does not parse.
#[test]
fn err_a_key_value_on_a_list_key_is_refused() {
    for (config, node) in [
        (
            "assets {\n  images {\n    responsive {\n      widths 480 960 foo=1\n    }\n  }\n}",
            "widths",
        ),
        (
            "check {\n  external {\n    accept 401 foo=1\n  }\n}",
            "accept",
        ),
        (
            "html {\n  anchors {\n    levels 2 3 four=9\n  }\n}",
            "levels",
        ),
        (
            "generate {\n  search {\n    stopwords \"a\" the=\"b\"\n  }\n}",
            "stopwords",
        ),
    ] {
        let rendered = err(config);
        assert!(
            rendered.contains("unexpected argument"),
            "{config}: {rendered}"
        );
        assert!(rendered.contains(node), "{config}: {rendered}");
    }
}

/// `typst { features }` is an open set with a reader of its own.
#[test]
fn err_a_key_value_among_typst_features_is_refused() {
    let rendered = err("typst {\n  features \"math\" pdf=#true\n}");
    assert!(rendered.contains("unexpected argument"), "{rendered}");
    assert!(rendered.contains("pdf=#true"), "{rendered}");
}

/// The one list whose reader owns its whole line keeps its own message, which
/// names the grammar it does take.
#[test]
fn a_toggled_list_speaks_for_itself() {
    let rendered =
        err("content {\n  markdown {\n    extensions \"tables\" footnotes=#true\n  }\n}");
    assert!(rendered.contains("unexpected argument"), "{rendered}");
    assert!(rendered.contains("footnotes=#true"), "{rendered}");
}

/// `highlight { enabled }` is not a key, so the flag on its line is the only
/// spelling that turns highlighting back off.
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
}

#[test]
fn err_a_class_for_no_token_is_refused_at_the_word() {
    let rendered = err("html {\n  highlight {\n    classes {\n      kewyord \"kw\"\n    }\n  }\n}");
    assert!(rendered.contains("kewyord"), "{rendered}");
    assert!(rendered.contains("keyword"), "did you mean: {rendered}");
}

#[test]
fn err_the_retired_scope_table_says_so() {
    let rendered = err("html {\n  highlight {\n    keyword \"#e5d004\"\n  }\n}");
    assert!(rendered.contains("keyword"), "{rendered}");
    assert!(rendered.contains("tokens"), "{rendered}");
}

/// The rule is arity, not prohibition: every shape a key legitimately takes has
/// to keep parsing.
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

/// A list that *replaces* what the key holds reads a bare node as the empty
/// list, which is the only spelling a profile has for undoing an inherited one.
#[test]
fn a_bare_list_key_is_the_empty_list_where_a_list_replaces() {
    assert!(parse("serve {\n  exclude\n}").serve.exclude.is_empty());
    assert!(
        parse("assets {\n  images {\n    responsive {\n      widths\n    }\n  }\n}")
            .assets
            .images
            .responsive
            .widths
            .is_empty(),
        "even where the default is not empty"
    );
}

/// A list written in the `-name` grammar amends the key's defaults instead, so
/// one naming nothing would amend nothing.
#[test]
fn err_a_bare_list_key_is_refused_where_a_list_amends() {
    for config in [
        "typst {\n  features\n}",
        "content {\n  markdown {\n    extensions\n  }\n}",
    ] {
        let rendered = err(config);
        assert!(
            rendered.contains("missing argument"),
            "{config}: {rendered}"
        );
    }
}
