//! `check { external { } }`: the manners the outbound check runs with.

use std::time::Duration;

use super::{code, parse};

/// The settings behind `external` are untouched by which spelling turned it on.
#[test]
fn the_flag_spelling_still_leaves_the_defaults_alone() {
    let default = parse("");
    for text in ["check {\n  external\n}", "check {\n  external #true\n}"] {
        let cfg = parse(text);
        assert!(cfg.check.external.enabled, "{text}");
        assert_eq!(
            cfg.check.external.fresh, default.check.external.fresh,
            "{text}"
        );
        assert_eq!(
            cfg.check.external.timeout, default.check.external.timeout,
            "{text}"
        );
        assert!(cfg.check.external.ignore.is_empty(), "{text}");
    }
}

/// Every unit the duration grammar knows, and a bare number meaning seconds.
#[test]
fn durations_are_read_in_the_unit_they_are_written_in() {
    let cfg = parse("check {\n  external {\n    fresh \"7d\"\n    timeout \"1.5m\"\n  }\n}");
    assert_eq!(cfg.check.external.fresh, Duration::from_hours(7 * 24));
    assert_eq!(cfg.check.external.timeout, Duration::from_secs(90));

    let cfg = parse("check {\n  external {\n    timeout 45\n  }\n}");
    assert_eq!(cfg.check.external.timeout, Duration::from_secs(45));
}

#[test]
fn a_duration_that_is_not_one_is_refused() {
    assert_eq!(
        code("check {\n  external {\n    fresh \"soon\"\n  }\n}"),
        "baudelaire::config::bad_duration"
    );
    assert_eq!(
        code("check {\n  external {\n    timeout -5\n  }\n}"),
        "baudelaire::config::negative_count"
    );
}

#[test]
fn the_lists_and_the_pool_are_read() {
    let cfg = parse(
        "check {\n  external {\n    concurrency 4\n    ignore \"*.internal/**\" \"staging.test/**\"\n    accept 401 429\n  }\n}",
    );
    assert_eq!(cfg.check.external.concurrency, Some(4));
    assert_eq!(
        cfg.check.external.ignore,
        vec!["*.internal/**".to_owned(), "staging.test/**".to_owned()]
    );
    assert_eq!(cfg.check.external.accept, vec![401, 429]);
}

/// A pool of nothing would hang rather than throttle, and a status code outside
/// the range a status code lives in matches nothing at all.
#[test]
fn a_pool_of_none_and_a_status_that_is_not_one_are_refused() {
    for text in [
        "check {\n  external {\n    concurrency 0\n  }\n}",
        "check {\n  external {\n    accept 1000\n  }\n}",
        "check {\n  external {\n    accept 200 7\n  }\n}",
    ] {
        assert_eq!(code(text), "baudelaire::config::out_of_range", "{text}");
    }
}

#[test]
fn a_profile_tunes_one_key_of_the_external_block() {
    let cfg = parse(
        r#"
        check { external { fresh "7d"; concurrency 8 } }
        profiles {
          ci {
            check { external { fresh "1h" } }
          }
        }
    "#,
    );
    let ci = cfg.with_profile("ci").expect("profile exists");
    assert_eq!(ci.check.external.fresh, Duration::from_hours(1));
    assert_eq!(ci.check.external.concurrency, Some(8), "siblings inherited");
    assert!(ci.check.external.enabled);
}
