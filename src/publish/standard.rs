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

use owo_colors::OwoColorize;

use crate::atproto::{AtUri, Blob, Did, Nsid, Repo, Rkey, Session};
use crate::config::{BaseUrl, StandardConfig};
use crate::error::warning::{DidUnpinned, Undated};
use crate::error::{PublishError, Result};
use crate::graph::{Fingerprint, Hash};
use crate::mime::Mime;
use crate::ui::Ui;

use super::{Doc, Options, Publisher, SiteView, SkipCache};

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

/// The standard.site backend, configured from a `publish { standard { .. } }`
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

    fn publish(&self, site: &SiteView, opts: &Options, ui: &Ui) -> Result<()> {
        if self.config.handle.is_empty() {
            return Err(PublishError::Unconfigured.into());
        }
        let base = site.config.base().ok_or(PublishError::NoUrl)?;

        let target = self.connect(opts, ui)?;
        if let Some(advice) = reconcile(self.config.did.as_deref(), target.did())? {
            ui.advice(advice);
        }
        let publication = publication_uri(target.did().as_str());

        // The publication record comes first, so documents can point at it — a
        // preview only diffs, so it writes nothing here.
        if let Target::Live(session) = &target {
            let record = Publication::new(site, &base, self.icon(session)?, self.config.discover);
            session.put_record(PUBLICATION, &Rkey::literal(PUBLICATION_RKEY), &record)?;
        }

        self.reconcile_documents(site, &target, &publication, ui)
    }
}

impl Standard {
    /// Connect to the publish target. A dry run resolves a read-only [`Repo`]
    /// from the handle without credentials (`listRecords` and `resolveHandle`
    /// are public XRPC); a real run authenticates a writable [`Session`] with the
    /// app password. Either way the resolved DID flows through [`reconcile`], so
    /// the identity check is the same on both paths.
    fn connect(&self, opts: &Options, ui: &Ui) -> Result<Target> {
        if opts.dry_run {
            ui.detail("dry run — no records will be written");
            let repo = Repo::resolve(&self.config.pds, &self.config.handle)?;
            return Ok(Target::Preview(repo));
        }
        let password = opts.secret(PASSWORD_ENV, "standard.site app password")?;
        let session = Session::login(&self.config.pds, &self.config.handle, &password)?;
        Ok(Target::Live(session))
    }

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

    /// Reconcile the site's dated pages with the document records in the repo:
    /// put new/changed records, skip unchanged, and delete records whose page is
    /// gone. Undated pages are not documents (standard.site requires a
    /// publication date) and are reported as skipped. A preview [`Target`] runs
    /// the same diff but writes nothing.
    fn reconcile_documents(
        &self,
        site: &SiteView,
        target: &Target,
        publication: &AtUri,
        ui: &Ui,
    ) -> Result<()> {
        let mut cache = SkipCache::load(self.name());
        let remote: BTreeSet<String> = target
            .repo()
            .list_rkeys(DOCUMENT)?
            .into_iter()
            .map(|rkey| rkey.as_str().to_owned())
            .collect();

        let mut desired = BTreeSet::new();
        let (mut sent, mut unchanged) = (0usize, 0usize);
        let mut undated: Vec<&str> = Vec::new();
        for doc in &site.documents {
            // Undated pages are not documents — standard.site requires a
            // `publishedAt` — so they are skipped and reported.
            let Some(date) = doc.date else {
                undated.push(&doc.path);
                continue;
            };
            let record = Document::new(doc, publication, date);
            let rkey = Rkey::derived(&doc.path);
            desired.insert(rkey.as_str().to_owned());
            let digest = record.fingerprint();
            // The cache alone is not authority: a record deleted on the PDS
            // out-of-band must be re-sent even if its fingerprint still matches.
            if remote.contains(rkey.as_str()) && cache.unchanged(rkey.as_str(), &digest) {
                unchanged += 1;
                continue;
            }
            if let Some(session) = target.writer() {
                session.put_record(DOCUMENT, &rkey, &record)?;
                cache.set(rkey.as_str().to_owned(), digest);
            }
            sent += 1;
        }

        let mut removed = 0usize;
        for stale in remote.difference(&desired) {
            if let Some(session) = target.writer() {
                session.delete_record(DOCUMENT, &Rkey::parsed(stale))?;
            }
            removed += 1;
        }

        // A preview computes the plan against the real remote but changes
        // nothing — locally or otherwise — so the skip-cache is left untouched.
        if !target.is_preview() {
            cache.retain(&desired);
            cache.save(self.name())?;
        }

        if !undated.is_empty() {
            // Each skipped page is listed at verbose; the typed warning always
            // carries the count.
            for path in &undated {
                ui.skip(path, "no publication date");
            }
            ui.warn(Undated {
                count: undated.len(),
            });
        }
        ui.done(Summary {
            name: self.name(),
            sent,
            unchanged,
            removed,
            preview: target.is_preview(),
        });
        Ok(())
    }
}

/// The repository a publish acts on, and how. A dry run gets a read-only
/// [`Repo`] resolved without credentials; a real run gets an authenticated
/// [`Session`] that can also write. Bundling each mode with its capability makes
/// an illegal combination — writing during a preview — unrepresentable, and
/// leaves one reconcile path to serve both.
enum Target {
    /// A dry run: read the live records, write nothing.
    Preview(Repo),
    /// An authenticated run: read and write.
    Live(Session),
}

impl Target {
    /// The read view, for diffing against the live records.
    fn repo(&self) -> &Repo {
        match self {
            Self::Preview(repo) => repo,
            Self::Live(session) => session.repo(),
        }
    }

    /// The repository DID identifying whose records this run reconciles.
    fn did(&self) -> &Did {
        self.repo().did()
    }

    /// The writer, present only for a live run — a preview writes nothing.
    fn writer(&self) -> Option<&Session> {
        match self {
            Self::Live(session) => Some(session),
            Self::Preview(_) => None,
        }
    }

    /// Whether this run only previews the plan.
    fn is_preview(&self) -> bool {
        matches!(self, Self::Preview(_))
    }
}

/// Reconcile the configured `did` pin against the identity a publish `resolved`.
/// A pin that disagrees is fatal — the build emitted verification artifacts for
/// the wrong account. No pin is fine, but the resolved DID comes back as
/// [`DidUnpinned`] advice so the user can pin it and get those artifacts;
/// `Ok(None)` means the pin held. Pure — the caller surfaces the advice — so it
/// is testable without a `Ui` or a network.
fn reconcile(pin: Option<&str>, resolved: &Did) -> Result<Option<DidUnpinned>, PublishError> {
    match pin {
        Some(did) if did == resolved.as_str() => Ok(None),
        Some(did) => Err(PublishError::DidMismatch {
            configured: did.to_owned(),
            actual: resolved.to_string(),
        }),
        None => Ok(Some(DidUnpinned {
            did: resolved.to_string(),
        })),
    }
}

/// A colored one-line publish summary: the destination, then counts styled by
/// meaning — sent in green (additive), unchanged dimmed (no-op), removed in
/// yellow when any went (else dimmed). `--dry-run` phrases the verbs as intent.
/// A [`Display`] newtype like [`Count`], so the styling lives in one place.
struct Summary<'a> {
    name: &'a str,
    sent: usize,
    unchanged: usize,
    removed: usize,
    preview: bool,
}

impl std::fmt::Display for Summary<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let (put, del) = if self.preview {
            ("to send", "to remove")
        } else {
            ("sent", "removed")
        };
        let sent = format!("{} {put}", self.sent).green().to_string();
        let same = format!("{} unchanged", self.unchanged).dimmed().to_string();
        let gone = format!("{} {del}", self.removed);
        let gone = if self.removed > 0 {
            gone.yellow().to_string()
        } else {
            gone.dimmed().to_string()
        };
        write!(f, "{} · {sent} · {same} · {gone}", self.name.cyan().bold())
    }
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

impl Publication {
    /// The site's publication record. The display name falls back to the site's
    /// label when `site` is unset.
    fn new(site: &SiteView, base: &BaseUrl, icon: Option<Blob>, discover: bool) -> Self {
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

impl Fingerprint for Document {
    /// The record's structural digest, keyed into the skip-cache so an unchanged
    /// document is not re-sent.
    fn fingerprint(&self) -> Hash {
        Hash::of(self)
    }
}

impl Document {
    /// A document record for a `doc` published on `date`, under `publication`.
    /// The caller filters undated pages — standard.site requires `publishedAt`.
    fn new(doc: &Doc, publication: &AtUri, date: time::Date) -> Self {
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

/// A date as an RFC 3339 timestamp at midnight UTC — the format `publishedAt`
/// requires. A [`Display`] adapter over [`Iso`](crate::content::Iso), so it
/// formats on demand without allocating a field.
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
    fn reconcile_accepts_a_matching_pin() {
        assert!(
            reconcile(Some("did:plc:x"), &Did::new("did:plc:x"))
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn reconcile_rejects_a_mismatched_pin() {
        assert!(matches!(
            reconcile(Some("did:plc:x"), &Did::new("did:plc:y")),
            Err(PublishError::DidMismatch { .. })
        ));
    }

    #[test]
    fn reconcile_advises_pinning_when_unset() {
        let advice = reconcile(None, &Did::new("did:plc:x")).unwrap();
        assert_eq!(advice.unwrap().did, "did:plc:x");
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

    fn summary(name: &str, sent: usize, unchanged: usize, removed: usize, preview: bool) -> String {
        Summary {
            name,
            sent,
            unchanged,
            removed,
            preview,
        }
        .to_string()
    }

    #[test]
    fn summary_names_the_destination_and_counts() {
        let line = summary("standard.site", 3, 1, 2, false);
        assert!(line.contains("standard.site"), "{line}");
        assert!(
            line.contains("3 sent") && line.contains("1 unchanged") && line.contains("2 removed"),
            "{line}"
        );
    }

    #[test]
    fn summary_dry_run_phrases_intent() {
        let line = summary("standard.site", 3, 0, 2, true);
        assert!(
            line.contains("3 to send") && line.contains("2 to remove"),
            "{line}"
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
