//! The defaults a key's doc string spells out, against the code that applies
//! them.

use crate::config::RedirectConfig;
use crate::config::content::entities::Unknown;
use crate::config::reference::Reference;
use crate::config::{FeedKind, Named, SortKey};
use crate::content::entities::Resolved;

/// One doc string that names its default, and what the code actually does.
type Claim = (&'static str, fn() -> Vec<String>);

/// The doc is what `config explain` prints and what `just reference` bakes into
/// the docs site, and neither compares it with the `Default` impl: a changed
/// default otherwise ships wrong documentation out of a green build.
const CLAIMED: &[Claim] = &[
    ("redirects.rules.status", || {
        vec![RedirectConfig::PERMANENT.to_string()]
    }),
    ("generate.feed.names.rss", || {
        vec![FeedKind::Rss.file().to_owned()]
    }),
    ("generate.feed.names.atom", || {
        vec![FeedKind::Atom.file().to_owned()]
    }),
    ("generate.feed.names.json", || {
        vec![FeedKind::Json.file().to_owned()]
    }),
    ("content.taxonomies.sort", || {
        vec![SortKey::Title.name().to_owned()]
    }),
    ("content.entities.slots.display", || {
        Resolved::FALLBACK.iter().map(|f| (*f).to_owned()).collect()
    }),
    ("content.entities.unknown", || {
        vec![
            Unknown::Error.name().to_owned(),
            Unknown::Synthesize.name().to_owned(),
        ]
    }),
];

/// The values a doc string names after "Defaults to", in the order written.
fn spelled(doc: &str) -> Vec<String> {
    let Some((_, rest)) = doc.split_once("Defaults to") else {
        return Vec::new();
    };
    rest.split('`')
        .skip(1)
        .step_by(2)
        .map(str::to_owned)
        .collect()
}

#[test]
fn a_doc_that_names_a_default_names_the_one_the_code_applies() {
    let reference = Reference::new();
    for (path, code) in CLAIMED {
        let entry = reference
            .entries()
            .iter()
            .find(|entry| entry.path == *path)
            .unwrap_or_else(|| panic!("`{path}` is not a config key"));
        assert_eq!(
            spelled(entry.doc),
            code(),
            "`{path}` documents `{}`",
            entry.doc
        );
    }
}

/// A new doc spelling a default out is only guarded once it is in `CLAIMED`;
/// a default described in prose ("the site title") names no value and is
/// nothing this can check.
#[test]
fn every_doc_that_spells_a_default_out_is_claimed() {
    for entry in Reference::new().entries() {
        if spelled(entry.doc).is_empty() {
            continue;
        }
        assert!(
            CLAIMED.iter().any(|(path, _)| *path == entry.path),
            "`{}` spells its default out with nothing checking it",
            entry.path
        );
    }
}
