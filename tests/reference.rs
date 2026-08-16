//! The generated config reference is checked in, and this is what keeps it
//! honest.

use baudelaire::config::reference::{Module, Reference};

/// Relative to the crate root, which is where cargo runs an integration test.
const PATH: &str = "docs/generated/reference.typ";

/// Regenerate with `just reference`, which is this test with `BLESS=1`.
#[test]
fn the_checked_in_reference_matches_the_dispatch_tables() {
    let expected = Module(&Reference::new()).to_string();

    if std::env::var_os("BLESS").is_some() {
        std::fs::write(PATH, &expected).expect("write the reference");
        return;
    }

    let actual = std::fs::read_to_string(PATH).unwrap_or_default();
    assert_eq!(
        actual, expected,
        "{PATH} is out of date with the config dispatch tables; run `just reference`"
    );
}

#[test]
fn the_walk_descends_into_nested_blocks() {
    let reference = Reference::new();
    let paths = reference.paths();

    for expected in [
        "paths.content",
        "assets.images.responsive.widths",
        "assets.images.optimize.png.level",
        "generate.feed.limit",
        "deploy.ssh.key",
        "content.collections.paginate.size",
    ] {
        assert!(paths.contains(&expected), "missing {expected}");
    }
}

/// A profile accepts every top-level key, including `profiles` itself, so
/// walking into it would not terminate.
#[test]
fn profiles_is_listed_without_recursing() {
    let reference = Reference::new();
    let paths = reference.paths();

    assert!(paths.contains(&"profiles"));
    assert!(
        !paths.iter().any(|p| p.starts_with("profiles.")),
        "profiles recursed: {:?}",
        paths
            .iter()
            .filter(|p| p.starts_with("profiles."))
            .collect::<Vec<_>>()
    );
}

#[test]
fn narrowing_returns_only_that_subtree() {
    let narrowed = Reference::at("deploy.s3").expect("deploy.s3 exists");
    let entries = narrowed.entries();

    assert_eq!(entries[0].path, "deploy.s3");
    assert_eq!(entries[0].depth, 0, "the named key is re-based to the root");
    assert!(
        entries[1..]
            .iter()
            .all(|e| e.path.starts_with("deploy.s3.")),
        "a sibling leaked in"
    );
    assert!(entries.iter().any(|e| e.key == "bucket"));
    assert!(!entries.iter().any(|e| e.key == "host"));
}

/// "No such key" and "this key has no settings" are different answers, and the
/// CLI turns only the first into an error.
#[test]
fn an_unknown_path_is_absent_rather_than_empty() {
    assert!(Reference::at("assets.imgs").is_none());
    assert!(Reference::at("").is_none());
}

#[test]
fn every_key_carries_a_description() {
    for entry in Reference::new().entries() {
        assert!(
            !entry.doc.trim().is_empty(),
            "{} has no description",
            entry.path
        );
        assert!(
            entry.doc.ends_with('.'),
            "{} should read as a sentence: {:?}",
            entry.path,
            entry.doc
        );
    }
}
