//! The [standard.site] record shapes: the lexicon ids, the `at://` URIs a
//! record lives at, and the JSON each serializes to.
//!
//! Kept apart from the backend that writes them, because the build reads them
//! too: an engine processor emits `.well-known` verification from
//! [`publication_uri`], a render transform emits a `<link>` from
//! [`document_uri`], and neither goes near a PDS. This module is the one
//! definition of the shapes both sides agree on.
//!
//! [standard.site]: https://standard.site

use serde::Serialize;

use crate::atproto::{AtUri, Blob, Did, Nsid, Rkey};
use crate::config::BaseUrl;
use crate::graph::{Fingerprint, Hash};

use super::super::{Doc, SiteView};

/// The lexicon ids, which double as repository collection names. Public so the
/// build-time verification artifacts (an engine processor for `.well-known`, a
/// render transform for `<link>` tags) share this one source of the record
/// shapes instead of re-spelling the NSIDs and key scheme.
pub const PUBLICATION: Nsid = Nsid::new("site.standard.publication");
pub const DOCUMENT: Nsid = Nsid::new("site.standard.document");
/// The conventional single record key for a site's publication.
pub(super) const PUBLICATION_RKEY: &str = "self";

/// The publication record's `at://` URI under `did`: the single definition of
/// where a site's publication lives, shared by announcing and verification.
pub fn publication_uri(did: &str) -> AtUri {
    AtUri::new(Did::new(did), PUBLICATION, Rkey::literal(PUBLICATION_RKEY))
}

/// A document's `at://` URI under `did`, keyed by its page `path`. The key is a
/// pure function of the path (see [`Rkey::derived`]), so the build names the
/// same record the backend writes, without any coordination.
pub fn document_uri(did: &str, path: &str) -> AtUri {
    AtUri::new(Did::new(did), DOCUMENT, Rkey::derived(path))
}

/// A `site.standard.publication` record.
#[derive(Serialize)]
pub(super) struct Publication {
    #[serde(rename = "$type")]
    kind: &'static str,
    name: String,
    url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    icon: Option<Blob>,
    preferences: Preferences,
}

impl Publication {
    /// The site's publication record. The display name falls back to the site's
    /// label when `site` is unset.
    pub(super) fn new(site: &SiteView, base: &BaseUrl, icon: Option<Blob>, discover: bool) -> Self {
        Self {
            kind: PUBLICATION.as_str(),
            name: site
                .config
                .site
                .clone()
                .unwrap_or_else(|| site.config.label().to_owned()),
            url: base.to_string(),
            description: None,
            icon,
            preferences: Preferences {
                show_in_discover: discover,
            },
        }
    }
}

/// Publication-level reader preferences.
#[derive(Serialize)]
struct Preferences {
    #[serde(rename = "showInDiscover")]
    show_in_discover: bool,
}

/// A `site.standard.document` record.
#[derive(Serialize, Hash)]
pub(super) struct Document {
    #[serde(rename = "$type")]
    kind: &'static str,
    site: AtUri,
    title: String,
    #[serde(rename = "publishedAt")]
    published_at: String,
    path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tags: Vec<String>,
}

impl Fingerprint for Document {
    /// The record's structural digest, keyed into the skip-cache so an unchanged
    /// document is not re-sent.
    fn fingerprint(&self) -> Hash {
        Hash::of(self)
    }
}

impl Document {
    /// A document record for a `doc` published on `date`, under `publication`.
    /// The caller filters undated pages: standard.site requires `publishedAt`.
    pub(super) fn new(doc: &Doc, publication: &AtUri, date: time::Date) -> Self {
        Self {
            kind: DOCUMENT.as_str(),
            site: publication.clone(),
            title: doc.title.clone(),
            published_at: Rfc3339(date).to_string(),
            path: doc.path.clone(),
            description: doc.description.clone(),
            tags: doc.tags.clone(),
        }
    }
}

/// A date as an RFC 3339 timestamp at midnight UTC: the format `publishedAt`
/// requires. A [`Display`](std::fmt::Display) adapter over
/// [`Iso`](crate::content::Iso), so it formats on demand without allocating a
/// field.
struct Rfc3339(time::Date);

impl std::fmt::Display for Rfc3339 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}T00:00:00Z", crate::content::Iso(self.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn date(y: i32, m: u8, d: u8) -> time::Date {
        time::Date::from_calendar_date(y, time::Month::try_from(m).unwrap(), d).unwrap()
    }

    fn sample(date: Option<time::Date>) -> Doc {
        Doc {
            path: "/posts/hi/".into(),
            title: "Hi".into(),
            description: Some("a post".into()),
            date,
            tags: vec!["rust".into()],
        }
    }

    #[test]
    fn uris_have_the_canonical_shape() {
        assert_eq!(
            publication_uri("did:plc:x").to_string(),
            "at://did:plc:x/site.standard.publication/self"
        );
        assert!(
            document_uri("did:plc:x", "/a/")
                .to_string()
                .starts_with("at://did:plc:x/site.standard.document/")
        );
    }

    #[test]
    fn rfc3339_is_midnight_utc() {
        assert_eq!(
            Rfc3339(date(2026, 7, 1)).to_string(),
            "2026-07-01T00:00:00Z"
        );
    }

    #[test]
    fn document_serializes_to_the_lexicon_shape() {
        let publication = publication_uri("did:plc:x");
        let record = Document::new(&sample(None), &publication, date(2026, 1, 2));
        let value = serde_json::to_value(&record).unwrap();
        assert_eq!(value["$type"], "site.standard.document");
        assert_eq!(
            value["site"],
            "at://did:plc:x/site.standard.publication/self"
        );
        assert_eq!(value["title"], "Hi");
        assert_eq!(value["publishedAt"], "2026-01-02T00:00:00Z");
        assert_eq!(value["path"], "/posts/hi/");
        assert_eq!(value["description"], "a post");
        assert_eq!(value["tags"], serde_json::json!(["rust"]));
    }

    #[test]
    fn document_omits_absent_optionals() {
        let publication = publication_uri("did:plc:x");
        let mut doc = sample(None);
        doc.description = None;
        doc.tags.clear();
        let value =
            serde_json::to_value(Document::new(&doc, &publication, date(2026, 1, 2))).unwrap();
        assert!(value.get("description").is_none());
        assert!(value.get("tags").is_none());
    }

    #[test]
    fn publication_serializes_to_the_lexicon_shape() {
        let record = Publication {
            kind: PUBLICATION.as_str(),
            name: "Site".into(),
            url: "https://example.com".into(),
            description: None,
            icon: None,
            preferences: Preferences {
                show_in_discover: true,
            },
        };
        let value = serde_json::to_value(&record).unwrap();
        assert_eq!(value["$type"], "site.standard.publication");
        assert_eq!(value["url"], "https://example.com");
        assert_eq!(value["preferences"]["showInDiscover"], true);
        assert!(value.get("icon").is_none());
        assert!(value.get("description").is_none());
    }
}
