//! The [standard.site] announcing backend.
//!
//! Maps a [`SiteView`] onto AT Protocol records (one `site.standard.publication`
//! for the site and one `site.standard.document` per dated page, both shaped by
//! [`record`]) and writes them to a PDS over XRPC. The remote repository is the
//! source of truth: every announce lists the existing document records and
//! deletes those no longer backed by a page, so nothing is orphaned. A local
//! [`SkipCache`] only spares re-sending records whose content is unchanged.
//!
//! Everything standard.site-specific lives here; the [`super`] layer stays
//! protocol-neutral.
//!
//! [standard.site]: https://standard.site

mod record;

use std::collections::BTreeSet;

use owo_colors::OwoColorize;

use crate::atproto::{AtUri, Blob, Did, Repo, Rkey, Session};
use crate::config::StandardConfig;
use crate::error::announce::Stage;
use crate::error::warning::{DidUnpinned, PlaintextEndpoint, Undated};
use crate::error::{AnnounceError, Result};
use crate::mime::Mime;
use crate::ui::Ui;

use self::record::{Document, PUBLICATION_RKEY, Publication};
use super::{Backend, SiteView, SkipCache};
use crate::remote::Options;

pub use self::record::{DOCUMENT, PUBLICATION, document_uri, publication_uri};

/// Environment variable holding the app password (never stored in config).
const PASSWORD_ENV: &str = "BAUDELAIRE_ATPROTO_PASSWORD";

/// The standard.site backend, configured from a `announce { standard { .. } }`
/// block.
pub struct Standard {
    config: StandardConfig,
}

impl Standard {
    pub fn new(config: StandardConfig) -> Self {
        Self { config }
    }
}

impl Backend<SiteView<'_>> for Standard {
    fn name(&self) -> &'static str {
        "standard.site"
    }

    fn run(&self, site: &SiteView, opts: &Options, ui: &Ui) -> Result<()> {
        if self.config.handle.is_empty() {
            return Err(AnnounceError::Unconfigured.into());
        }
        let base = site.config.base().ok_or(AnnounceError::NoUrl)?;
        if let Some(warning) = Self::plaintext(&self.config.pds) {
            ui.warn(warning);
        }

        let target = self.connect(opts, ui)?;
        if let Some(advice) = Self::pinned(self.config.did.as_deref(), target.did())? {
            ui.advice(advice);
        }
        let publication = publication_uri(target.did().as_str());

        // The publication record comes first, so documents can point at it; a
        // preview only diffs, so it writes nothing here.
        if let Target::Live(session) = &target {
            let record = Publication::new(site, &base, self.icon(session)?, self.config.discover);
            session.put_record(PUBLICATION, &Rkey::literal(PUBLICATION_RKEY), &record)?;
        }

        self.reconcile_documents(site, &target, &publication, ui)
    }
}

impl Standard {
    /// Connect to the destination. A dry run resolves a read-only [`Repo`]
    /// from the handle without credentials (`listRecords` and `resolveHandle`
    /// are public XRPC); a real run authenticates a writable [`Session`] with the
    /// app password. Either way the resolved DID flows through
    /// [`Standard::pinned`], so the identity check is the same on both paths.
    fn connect(&self, opts: &Options, ui: &Ui) -> Result<Target> {
        if opts.dry_run {
            ui.detail("dry run: no records will be written");
            let repo = Repo::resolve(&self.config.pds, &self.config.handle)?;
            return Ok(Target::Preview(repo));
        }
        let password = opts.secret(PASSWORD_ENV, "standard.site app password")?;
        let session = Session::login(&self.config.pds, &self.config.handle, &password)?;
        Ok(Target::Live(session))
    }

    /// Check the configured `did` pin against the identity an announce
    /// `resolved`. A pin that disagrees is fatal: the build emitted verification
    /// artifacts for the wrong account. No pin is fine, but the resolved DID
    /// comes back as [`DidUnpinned`] advice so the user can pin it and get those
    /// artifacts; `Ok(None)` means the pin held. Takes the pin rather than
    /// reading it off `self`, so it is testable without a `Ui` or a network.
    fn pinned(pin: Option<&str>, resolved: &Did) -> Result<Option<DidUnpinned>, AnnounceError> {
        match pin {
            Some(did) if did == resolved.as_str() => Ok(None),
            Some(did) => Err(AnnounceError::DidMismatch {
                configured: did.to_owned(),
                actual: resolved.to_string(),
            }),
            None => Ok(Some(DidUnpinned {
                did: resolved.to_string(),
            })),
        }
    }

    /// Stop the run at `at`, keeping what it has already done.
    ///
    /// The skip-cache is written as it stands, *without* [`SkipCache::retain`]:
    /// the desired set is only as complete as the loop got, and pruning against
    /// a half-built one would disown records the run never reached. Its own
    /// failure is swallowed here and only here, because this path is already
    /// failing and the cost of not writing it is one repeated re-send.
    fn stopped(
        &self,
        cache: &SkipCache,
        stage: Stage,
        done: usize,
        total: usize,
        at: &str,
        why: AnnounceError,
    ) -> crate::error::BaudelaireErrorKind {
        let _ = cache.save(self.name());
        AnnounceError::interrupted(stage, done, total, at, why).into()
    }

    /// Whether `pds` is spelled over plain HTTP, in which case the app password
    /// this run sends travels in clear on the wire.
    ///
    /// The scheme the author typed, not the transport: TLS is configured and
    /// verified for every `https://` request (see [`crate::remote::Http`]). It
    /// is `http://` in `config.kdl` that quietly opts out of all of it.
    /// Reported rather than refused, because a PDS on `localhost` is how the
    /// protocol is developed against.
    fn plaintext(pds: &str) -> Option<PlaintextEndpoint> {
        pds.starts_with("http://").then(|| PlaintextEndpoint {
            setting: "announce { standard { pds } }",
            url: pds.to_owned(),
            secret: "the app password",
        })
    }

    /// Upload the configured publication icon as a blob, if any. The path is
    /// resolved against the project root (the process cwd during an announce).
    fn icon(&self, session: &Session) -> Result<Option<Blob>> {
        let Some(path) = &self.config.icon else {
            return Ok(None);
        };
        // `crate::fs` is what carries the path and the operation into the
        // diagnostic. This used to build an `FsError` by hand over a raw
        // `std::fs::read`, which is the same error with a second spelling.
        let bytes = crate::fs::read(path)?;
        Ok(Some(session.upload_blob(&bytes, Mime::of(path))?))
    }

    /// Reconcile the site's dated pages with the document records in the repo:
    /// put new/changed records, skip unchanged, and delete records whose page is
    /// gone. Undated pages are not documents (standard.site requires a
    /// publication date) and are reported as skipped. A preview [`Target`] runs
    /// the same diff but writes nothing.
    ///
    /// A write that fails partway stops the run through [`Standard::stopped`],
    /// which keeps what was already done: the loops here never `?` a write
    /// straight out, because that threw away both the skip-cache and any word
    /// about how far the run had got.
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
        let dated = site
            .documents
            .iter()
            .filter(|doc| doc.date.is_some())
            .count();
        for doc in &site.documents {
            // Undated pages are not documents (standard.site requires a
            // `publishedAt`), so they are skipped and reported.
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
                if let Err(why) = session.put_record(DOCUMENT, &rkey, &record) {
                    return Err(self.stopped(
                        &cache,
                        Stage::Send,
                        sent + unchanged,
                        dated,
                        &doc.path,
                        why,
                    ));
                }
                cache.set(rkey.as_str().to_owned(), digest);
            }
            sent += 1;
        }

        let stale: Vec<&str> = remote.difference(&desired).map(String::as_str).collect();
        let mut removed = 0usize;
        for rkey in stale.iter().copied() {
            if let Some(session) = target.writer()
                && let Err(why) = session.delete_record(DOCUMENT, &Rkey::parsed(rkey))
            {
                return Err(self.stopped(&cache, Stage::Remove, removed, stale.len(), rkey, why));
            }
            removed += 1;
        }

        // A preview computes the plan against the real remote but changes
        // nothing (locally or otherwise), so the skip-cache is left untouched.
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

/// The repository an announce acts on, and how. A dry run gets a read-only
/// [`Repo`] resolved without credentials; a real run gets an authenticated
/// [`Session`] that can also write. Bundling each mode with its capability makes
/// an illegal combination (writing during a preview) unrepresentable, and
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

    /// The writer, present only for a live run; a preview writes nothing.
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

/// A colored one-line announce summary: the destination, then counts styled by
/// meaning: sent in green (additive), unchanged dimmed (no-op), removed in
/// yellow when any went (else dimmed). `--dry-run` phrases the verbs as intent.
/// A [`Display`](std::fmt::Display) newtype like [`Count`](crate::ui::Count), so
/// the styling lives in one place.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pinned_accepts_a_matching_pin() {
        assert!(
            Standard::pinned(Some("did:plc:x"), &Did::new("did:plc:x"))
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn pinned_rejects_a_mismatched_pin() {
        assert!(matches!(
            Standard::pinned(Some("did:plc:x"), &Did::new("did:plc:y")),
            Err(AnnounceError::DidMismatch { .. })
        ));
    }

    #[test]
    fn pinned_advises_pinning_when_unset() {
        let advice = Standard::pinned(None, &Did::new("did:plc:x")).unwrap();
        assert_eq!(advice.unwrap().did, "did:plc:x");
    }

    /// An app password sent to an `http://` PDS travels in clear. TLS is
    /// configured and verified for everything else; the scheme in `config.kdl`
    /// is the one thing that opts out of it, and it used to do so in silence.
    #[test]
    fn a_plaintext_pds_is_reported() {
        assert!(Standard::plaintext("https://bsky.social").is_none());
        let warning = Standard::plaintext("http://pds.example.test").expect("warned");
        assert_eq!(warning.url, "http://pds.example.test");
        // A local PDS is how the protocol is developed against, so this warns
        // rather than refusing.
        assert!(Standard::plaintext("http://localhost:2583").is_some());
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
}
