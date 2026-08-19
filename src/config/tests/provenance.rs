//! What a config *holds*, and which layer it came from: the read half of the
//! dispatch tables, and the layers `config get` and `config explain` resolve.

use std::fmt::Write as _;

use super::parse;
use crate::config::Config;
use crate::config::dispatch::Kind;
use crate::config::reference::Reference;
use crate::config::values::Value;
use crate::config::values::source::{Layer, Sources};

/// Every key a fixed table declares is readable, so a row that parses a key can
/// never be one that reports nothing under it.
///
/// Read off a config that names every block, since a switchable section that is
/// off reads as its own flag and holds no keys.
#[test]
fn every_declared_key_reads_back() {
    let reference = Reference::new();
    let values = parse(&every_block()).values();
    let mut checked = 0;
    for entry in reference.entries() {
        if !addressable(&entry.path) {
            continue;
        }
        assert!(
            values.at(&entry.path).is_some(),
            "`{}` parses but reads nothing",
            entry.path
        );
        checked += 1;
    }
    assert!(checked > 100, "only {checked} keys were reachable");
}

/// Whether a path runs through blocks alone: a key under a name the author
/// chooses (a collection, a redirect) is only there once they have written one.
fn addressable(path: &str) -> bool {
    let mut walked = String::new();
    for step in path.split('.') {
        if !walked.is_empty() {
            walked.push('.');
        }
        walked.push_str(step);
        if walked == path {
            break;
        }
        let Some(parent) = Reference::at(&walked) else {
            return false;
        };
        if !matches!(parent.entries()[0].kind, Kind::Block(_)) {
            return false;
        }
    }
    true
}

#[test]
fn a_key_reads_the_value_its_own_line_writes() {
    let config = parse("lint {\n  headings {\n    start 3\n  }\n}");
    let values = config.values();
    assert_eq!(values.at("lint.headings.start"), Some(&Value::Number(3)));
    assert_eq!(
        values.at("paths.dist"),
        Some(&Value::Text("public".to_owned())),
        "a key nothing writes still reads its default"
    );
    assert_eq!(values.at("lint.headings.nothing"), None);
}

/// A section turned off carries the boolean its own line would, and its keys
/// stay readable: a later layer turning a section off must not leave an earlier
/// layer's value looking like the effective one.
#[test]
fn a_section_that_is_off_says_so_on_its_own_line() {
    let off = Config::default().values();
    let Some(Value::Node { args, keys, .. }) = off.at("lint") else {
        panic!("a section reads as a node");
    };
    assert_eq!(args, &vec![Value::Flag(false)]);
    assert!(!keys.is_empty(), "and still holds its keys");

    let on = parse("lint {\n  strict\n}");
    assert_eq!(on.values().at("lint.strict"), Some(&Value::Flag(true)));
}

#[test]
fn a_value_comes_from_the_last_layer_that_changed_it() {
    let text = "paths {\n  dist \"out\"\n}\nprofiles {\n  dev {\n    paths {\n      dist \"dev\"\n    }\n  }\n}";
    let root = std::path::Path::new(".");

    let sources = Sources::of(text, root, None, None).expect("parses");
    let resolved = sources.resolve("paths.dist").expect("a held key");
    assert_eq!(resolved.value, &Value::Text("out".to_owned()));
    assert_eq!(resolved.layer(), &Layer::File);
    assert_eq!(resolved.trail.len(), 2, "the default it replaced is kept");
    assert_eq!(resolved.trail[0].0, &Layer::Default);

    let sources = Sources::of(text, root, None, Some("dev")).expect("parses");
    let resolved = sources.resolve("paths.dist").expect("a held key");
    assert_eq!(resolved.value, &Value::Text("dev".to_owned()));
    assert_eq!(resolved.layer(), &Layer::Profile("dev".to_owned()));
}

/// A key no layer writes still resolves, to the default it started at, and its
/// trail names that one layer alone.
#[test]
fn an_untouched_key_comes_from_the_defaults() {
    let sources = Sources::of("site \"T\"", std::path::Path::new("."), None, None).expect("parses");
    let resolved = sources.resolve("content.future").expect("a held key");
    assert_eq!(resolved.layer(), &Layer::Default);
    assert_eq!(resolved.trail.len(), 1);
}

/// The tree is written back as the config that would parse to it, so what a
/// reader is shown is a config line and not a debug rendering.
#[test]
fn a_block_is_written_back_as_kdl() {
    let config =
        parse("lint {\n  headings {\n    start 3\n  }\n  budget {\n    html \"1kB\"\n  }\n}");
    let written = config
        .values()
        .at("lint.budget")
        .expect("a held block")
        .scalar();
    assert!(written.contains("html 1024"), "{written}");
    assert!(!written.contains("js"), "a key holding nothing is left out");
}

/// A config naming every block the tables declare, so every switchable section
/// is on and every key it holds is reachable.
fn every_block() -> String {
    let reference = Reference::new();
    let mut paths: Vec<&str> = reference
        .entries()
        .iter()
        .filter(|entry| matches!(entry.kind, Kind::Block(_)) && addressable(&entry.path))
        .map(|entry| entry.path.as_str())
        .collect();
    paths.sort_unstable();
    let mut written = String::new();
    let mut open: Vec<&str> = Vec::new();
    for path in paths {
        let steps: Vec<&str> = path.split('.').collect();
        while !open.is_empty() && !steps.starts_with(&open) {
            open.pop();
            let _ = writeln!(written, "{}}}", "  ".repeat(open.len()));
        }
        let _ = writeln!(
            written,
            "{}{} {{",
            "  ".repeat(open.len()),
            steps[steps.len() - 1]
        );
        open = steps;
    }
    while !open.is_empty() {
        open.pop();
        let _ = writeln!(written, "{}}}", "  ".repeat(open.len()));
    }
    written
}

/// A block is written back with its own line's arguments, and with a key the
/// parser would not read back bare in quotes: `/old/*` written bare opens a
/// comment in everything printed after it.
#[test]
fn a_block_carries_its_arguments_and_quotes_the_keys_that_need_it() {
    let config = parse("redirects {\n  rules {\n    \"/latest/*\" \"/:splat\" status=302\n  }\n}");
    let written = config
        .values()
        .at("redirects.rules")
        .expect("a held block")
        .scalar();
    assert!(
        written.starts_with("\"/latest/*\" \"/:splat\" status=302"),
        "{written}"
    );

    let config = parse("lint {\n  snippets {\n    kdl \"warn\"\n  }\n}");
    let written = config
        .values()
        .at("lint.snippets")
        .expect("a held block")
        .scalar();
    assert!(written.starts_with("kdl \"warn\""), "{written}");
}
