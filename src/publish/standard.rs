//! The [standard.site] publishing backend.
//!
//! Maps a [`SiteView`] onto AT Protocol records — one `site.standard.publication`
//! for the site and one `site.standard.document` per dated page — and writes them
//! to a PDS over XRPC. The remote repository is the source of truth: every
//! publish lists the existing document records and deletes those no longer
//! backed by a page, so nothing is orphaned. A local [`SkipCache`] only spares
//! re-sending records whose content is unchanged.
//!
//! Everything standard.site-specific lives here; the [`super`] layer stays
//! protocol-neutral.
//!
//! [standard.site]: https://standard.site

use std::collections::BTreeSet;

use serde::Serialize;

use crate::atproto::{AtUri, Blob, Did, Nsid, Rkey, Session};
use crate::cli::output::Report;
use crate::config::StandardConfig;
use crate::error::{PublishError, Result};
use crate::mime::Mime;

use super::{Doc, Publisher, SiteView, SkipCache};

/// The lexicon ids, which double as repository collection names. Public so the
/// build-time verification artifacts (an engine processor for `.well-known`, a
/// render transform for `<link>` tags) share this one source of the record
/// shapes instead of re-spelling the NSIDs and key scheme.
pub const PUBLICATION: Nsid = Nsid::new("site.standard.publication");
pub const DOCUMENT: Nsid = Nsid::new("site.standard.document");
/// The conventional single record key for a site's publication.
const PUBLICATION_RKEY: &str = "self";
/// Environment variable holding the app password (never stored in config).
const PASSWORD_ENV: &str = "BAUDELAIRE_ATPROTO_PASSWORD";

/// The publication record's `at://` URI under `did` — the single definition of
/// where a site's publication lives, shared by publishing and verification.
pub fn publication_uri(did: &str) -> AtUri {
    AtUri::new(Did::new(did), PUBLICATION, Rkey::literal(PUBLICATION_RKEY))
}

/// A document's `at://` URI under `did`, keyed by its page `path`. The key is a
/// pure function of the path (see [`Rkey::derived`]), so the build names the
/// same record the publisher writes, without any coordination.
pub fn document_uri(did: &str, path: &str) -> AtUri {
    AtUri::new(Did::new(did), DOCUMENT, Rkey::derived(path))
}

/// The standard.site backend, configured from a `publish { standard { … } }`
/// block.
pub struct Standard {
    config: StandardConfig,
}

impl Standard {
    pub fn new(config: StandardConfig) -> Self {
        Self { config }
    }
}

impl Publisher for Standard {
    fn name(&self) -> &'static str {
        "standard.site"
    }

    fn publish(&self, site: &SiteView, report: &mut Report) -> Result<()> {
        if self.config.handle.is_empty() {
            return Err(PublishError::Unconfigured.into());
        }
        let base = site.config.base().ok_or(PublishError::NoUrl)?;
        let password =
            std::env::var(PASSWORD_ENV).map_err(|_| PublishError::NoPassword)?;

        let session = Session::login(&self.config.pds, &self.config.handle, &password)?;
        self.check_did(session.did(), report)?;
        let publication = publication_uri(session.did().as_str());

        // The publication record first, so the documents can point at it.
        let icon = self.icon(&session)?;
        session.put_record(PUBLICATION, &Rkey::literal(PUBLICATION_RKEY), &Publication {
            kind: PUBLICATION.as_str(),
            name: site.config.site.clone().unwrap_or_else(|| site.config.label().to_owned()),
            url: base.to_string(),
            description: None,
            icon,
            preferences: Preferences { show_in_discover: self.config.discover },
        })?;

        self.documents(site, &session, &publication, report)
    }
}

impl Standard {
    /// Upload the configured publication icon as a blob, if any. The path is
    /// resolved against the project root (the process cwd during a publish).
    fn icon(&self, session: &Session) -> Result<Option<Blob>> {
        let Some(path) = &self.config.icon else {
            return Ok(None);
        };
        let bytes = std::fs::read(path).map_err(|source| PublishError::Icon {
            path: path.display().to_string(),
            source,
        })?;
        Ok(Some(session.upload_blob(&bytes, Mime::of(path))?))
    }

    /// Reconcile the configured `did` with the one the session authenticated as:
    /// a mismatch means the build emitted verification artifacts for the wrong
    /// identity, so it is fatal; an unset `did` only forgoes those artifacts, so
    /// it is a note pointing at the value to configure.
    fn check_did(&self, actual: &Did, report: &mut Report) -> Result<()> {
        match &self.config.did {
            Some(did) if did == actual.as_str() => Ok(()),
            Some(did) => Err(PublishError::DidMismatch {
                configured: did.clone(),
                actual: actual.to_string(),
            }
            .into()),
            None => {
                report.info(format_args!(
                    "set `publish.standard.did \"{actual}\"` to emit verification artifacts at build"
                ))?;
                Ok(())
            }
        }
    }

    /// Reconcile the site's dated pages with the document records in the repo:
    /// put new/changed records, skip unchanged, and delete records whose page is
    /// gone. Undated pages are not documents (standard.site requires a
    /// publication date) and are reported as skipped.
    fn documents(
        &self,
        site: &SiteView,
        session: &Session,
        publication: &AtUri,
        report: &mut Report,
    ) -> Result<()> {
        let mut cache = SkipCache::load(self.name());
        let remote: BTreeSet<String> = session
            .list_rkeys(DOCUMENT)?
            .into_iter()
            .map(|rkey| rkey.as_str().to_owned())
            .collect();

        let mut desired = BTreeSet::new();
        let (mut sent, mut unchanged, mut undated) = (0usize, 0usize, 0usize);
        for doc in &site.documents {
            let Some(record) = Document::from_doc(doc, publication) else {
                undated += 1;
                continue;
            };
            let rkey = Rkey::derived(&doc.path);
            desired.insert(rkey.as_str().to_owned());
            let fingerprint = fingerprint(&record)?;
            if cache.unchanged(rkey.as_str(), &fingerprint) {
                unchanged += 1;
                continue;
            }
            session.put_record(DOCUMENT, &rkey, &record)?;
            cache.set(rkey.as_str().to_owned(), fingerprint);
            sent += 1;
        }

        let mut removed = 0usize;
        for stale in remote.difference(&desired) {
            session.delete_record(DOCUMENT, &Rkey::parsed(stale))?;
            removed += 1;
        }

        cache.retain(&desired);
        cache.save(self.name())?;

        if undated > 0 {
            report.warn(format_args!("{undated} undated page(s) skipped (no publication date)"))?;
        }
        report.success(format_args!(
            "standard.site: {sent} sent · {unchanged} unchanged · {removed} removed"
        ))?;
        Ok(())
    }
}

/// The blake3 fingerprint of a record's serialized form, for the skip-cache.
fn fingerprint(record: &impl Serialize) -> Result<String> {
    let bytes = serde_json::to_vec(record)
        .map_err(|e| crate::error::SerializeError::new(crate::error::Artifact::PublishCache, e))?;
    Ok(blake3::hash(&bytes).to_hex().to_string())
}

/// A `site.standard.publication` record.
#[derive(Serialize)]
struct Publication {
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

/// Publication-level reader preferences.
#[derive(Serialize)]
struct Preferences {
    #[serde(rename = "showInDiscover")]
    show_in_discover: bool,
}

/// A `site.standard.document` record.
#[derive(Serialize)]
struct Document {
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

impl Document {
    /// Build a document record from a neutral [`Doc`], or `None` when the page
    /// has no date — standard.site requires `publishedAt`.
    fn from_doc(doc: &Doc, publication: &AtUri) -> Option<Self> {
        let date = doc.date?;
        Some(Self {
            kind: DOCUMENT.as_str(),
            site: publication.clone(),
            title: doc.title.clone(),
            published_at: rfc3339(date),
            path: doc.path.clone(),
            description: doc.description.clone(),
            tags: doc.tags.clone(),
        })
    }
}

/// A date as an RFC 3339 timestamp at midnight UTC — the format `publishedAt`
/// requires. Built directly from the fields, so it never fails.
fn rfc3339(date: time::Date) -> String {
    format!(
        "{:04}-{:02}-{:02}T00:00:00Z",
        date.year(),
        u8::from(date.month()),
        date.day()
    )
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
        assert_eq!(rfc3339(date(2026, 7, 1)), "2026-07-01T00:00:00Z");
    }

    #[test]
    fn undated_page_is_not_a_document() {
        let publication = publication_uri("did:plc:x");
        assert!(Document::from_doc(&sample(None), &publication).is_none());
    }

    #[test]
    fn document_serializes_to_the_lexicon_shape() {
        let publication = publication_uri("did:plc:x");
        let record = Document::from_doc(&sample(Some(date(2026, 1, 2))), &publication).unwrap();
        let value = serde_json::to_value(&record).unwrap();
        assert_eq!(value["$type"], "site.standard.document");
        assert_eq!(value["site"], "at://did:plc:x/site.standard.publication/self");
        assert_eq!(value["title"], "Hi");
        assert_eq!(value["publishedAt"], "2026-01-02T00:00:00Z");
        assert_eq!(value["path"], "/posts/hi/");
        assert_eq!(value["description"], "a post");
        assert_eq!(value["tags"], serde_json::json!(["rust"]));
    }

    #[test]
    fn document_omits_absent_optionals() {
        let publication = publication_uri("did:plc:x");
        let mut doc = sample(Some(date(2026, 1, 2)));
        doc.description = None;
        doc.tags.clear();
        let value = serde_json::to_value(Document::from_doc(&doc, &publication).unwrap()).unwrap();
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
            preferences: Preferences { show_in_discover: true },
        };
        let value = serde_json::to_value(&record).unwrap();
        assert_eq!(value["$type"], "site.standard.publication");
        assert_eq!(value["url"], "https://example.com");
        assert_eq!(value["preferences"]["showInDiscover"], true);
        assert!(value.get("icon").is_none());
        assert!(value.get("description").is_none());
    }
}
