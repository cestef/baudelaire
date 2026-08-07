//! `links { external { } }`: the manners the outbound check runs with.

use std::time::Duration;

use super::{code, parse};

/// `external` was a flag, and the block has to stay the same flag: the spelling
/// every site already has, still meaning what it did. Turning it *on* is covered
/// by the switch table in `switches`; this is that the settings behind it are
/// untouched by which spelling turned it on.
#[test]
fn the_flag_spelling_still_leaves_the_defaults_alone() {
    let default = parse("");
    for text in ["links {\n  external\n}", "links {\n  external #true\n}"] {
        let cfg = parse(text);
        assert!(cfg.links.external.enabled, "{text}");
        assert_eq!(
            cfg.links.external.fresh, default.links.external.fresh,
            "{text}"
        );
        assert_eq!(
            cfg.links.external.timeout, default.links.external.timeout,
            "{text}"
        );
        assert!(cfg.links.external.ignore.is_empty(), "{text}");
    }
}

/// Every unit the duration grammar knows, and a bare number meaning seconds.
#[test]
fn durations_are_read_in_the_unit_they_are_written_in() {
    let cfg = parse("links {\n  external {\n    fresh \"7d\"\n    timeout \"1.5m\"\n  }\n}");
    assert_eq!(cfg.links.external.fresh, Duration::from_hours(7 * 24));
    assert_eq!(cfg.links.external.timeout, Duration::from_secs(90));

    let cfg = parse("links {\n  external {\n    timeout 45\n  }\n}");
    assert_eq!(cfg.links.external.timeout, Duration::from_secs(45));
}

#[test]
fn a_duration_that_is_not_one_is_refused() {
    assert_eq!(
        code("links {\n  external {\n    fresh \"soon\"\n  }\n}"),
        "baudelaire::config::bad_duration"
    );
    // A negative count, not a duration: the integer spelling is held to the
    // same rule every other count is.
    assert_eq!(
        code("links {\n  external {\n    timeout -5\n  }\n}"),
        "baudelaire::config::negative_count"
    );
}

#[test]
fn the_lists_and_the_pool_are_read() {
    let cfg = parse(
        "links {\n  external {\n    concurrency 4\n    ignore \"*.internal/**\" \"staging.test/**\"\n    accept 401 429\n  }\n}",
    );
    assert_eq!(cfg.links.external.concurrency, Some(4));
    assert_eq!(
        cfg.links.external.ignore,
        vec!["*.internal/**".to_owned(), "staging.test/**".to_owned()]
    );
    assert_eq!(cfg.links.external.accept, vec![401, 429]);
}

/// A pool of nothing would hang rather than throttle, and a status code outside
/// the range a status code lives in matches nothing at all. Both are typos, and
/// both are named as such.
#[test]
fn a_pool_of_none_and_a_status_that_is_not_one_are_refused() {
    for text in [
        "links {\n  external {\n    concurrency 0\n  }\n}",
        "links {\n  external {\n    accept 1000\n  }\n}",
        "links {\n  external {\n    accept 200 7\n  }\n}",
    ] {
        assert_eq!(code(text), "baudelaire::config::out_of_range", "{text}");
    }
}

/// A profile tuning one key inherits its siblings, the merge policy every
/// nested section has.
#[test]
fn a_profile_tunes_one_key_of_the_external_block() {
    let cfg = parse(
        r#"
        links { external { fresh "7d"; concurrency 8 } }
        profiles {
          ci {
            links { external { fresh "1h" } }
          }
        }
    "#,
    );
    let ci = cfg.with_profile("ci").expect("profile exists");
    assert_eq!(ci.links.external.fresh, Duration::from_hours(1));
    assert_eq!(ci.links.external.concurrency, Some(8), "siblings inherited");
    assert!(ci.links.external.enabled);
}
