//! Deploying the built site's *files* to a host, the counterpart to
//! [`crate::announce`], which publishes metadata records. A destination
//! implements one [`Backend`]; it receives the built [`Dist`] and reconciles the
//! remote with it (upload changed, delete removed). Reconciling itself is
//! [`Dist::reconcile`], shared by every destination: a backend only supplies the
//! [`Store`] it talks to. Adding a destination is one `impl Backend`, one `impl
//! Store`, plus one line in [`configured`]; nothing else learns about it.

mod s3;
mod sigv4;
#[cfg(feature = "ssh")]
mod ssh;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::config::Config;
use crate::error::{DeployError, Result};
use crate::remote::{self, Backend, Options};
use crate::ui::{Count, Marker, Ui};

/// Files keyed by dist-relative path, each mapped to a content digest. The
/// currency every [`Backend`] reconciles in: local digests versus remote.
pub type Digests = BTreeMap<String, String>;

/// A path the *remote* named, admitted only if it stays inside the deploy root.
///
/// Both backends learn the remote's file list from the remote itself: SSH parses
/// the host's own `sha256sum` output, S3 a `ListObjectsV2` response. Those paths
/// feed straight into a delete, so a hostile or broken host answering
/// `../../etc/nginx/sites-enabled/x` would have the client remove a file well
/// outside the deploy root, as whichever user it authenticated as. Every
/// remote-supplied path is filtered through here before it reaches a path join.
/// Constructed only by [`Listed::try_from`], so the check cannot be skipped:
/// every function that turns a remote path into a request takes one of these
/// rather than a `&str`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Listed(String);

impl TryFrom<&str> for Listed {
    type Error = ();

    /// Accepts a plain relative path confined to the root: not absolute, no
    /// `..`, no empty or bare-`.` component, no backslash (which a Windows-ish
    /// remote could use as a separator we do not split on).
    fn try_from(path: &str) -> Result<Self, Self::Error> {
        let confined = !path.is_empty()
            && !path.starts_with('/')
            && !path.contains('\\')
            && path
                .split('/')
                .all(|part| !part.is_empty() && part != "." && part != "..");
        confined.then(|| Self(path.to_owned())).ok_or(())
    }
}

impl Listed {
    /// Give up the checked path. Consuming, so a `Listed` cannot be checked
    /// once and then used twice over.
    pub fn into_string(self) -> String {
        self.0
    }
}

#[cfg(test)]
use crate::ui::Level;

use self::s3::S3;
#[cfg(feature = "ssh")]
use self::ssh::Ssh;

/// The built output tree handed to every [`Backend`]: the `dist` root and every
/// file under it as a forward-slashed, root-relative path. Bytes are read on
/// demand rather than held, so reconciling a large site stays streaming: a
/// backend hashes each file to decide what changed, then reads only what it
/// uploads.
pub struct Dist {
    root: PathBuf,
    /// Every file under `root`, as a sorted, forward-slashed relative path.
    pub files: Vec<String>,
}

impl Dist {
    /// Walk `root` into the set of relative file paths.
    ///
    /// Subdirectories are followed, but not out of `dist`: a symlink pointing
    /// elsewhere would otherwise have the deploy upload files the build never
    /// produced, the mirror image of what [`crate::engine::prune::Prune::owns`]
    /// refuses to delete.
    fn scan(root: &Path) -> Result<Self> {
        let contained = crate::fs::canonical(root);
        let mut files: Vec<String> = crate::fs::Walk::new(root)
            .skipping(|dir| !crate::fs::canonical(dir).starts_with(&contained))
            .files()?
            .iter()
            .filter_map(|path| path.strip_prefix(root).ok())
            .map(|rel| rel.to_string_lossy().replace('\\', "/"))
            .collect();
        files.sort();
        Ok(Self {
            root: root.to_owned(),
            files,
        })
    }

    /// Read one file's bytes by its relative path.
    pub fn read(&self, rel: &str) -> Result<Vec<u8>> {
        crate::fs::read(self.root.join(rel))
    }

    /// Digest every file with `hash`, keyed by relative path. Each file is read
    /// once, hashed, and dropped, so a large site never sits wholly in memory;
    /// the algorithm is the backend's choice (S3 wants MD5, SSH SHA-256).
    pub fn digests(&self, hash: impl Fn(&[u8]) -> String) -> Result<Digests> {
        self.files
            .iter()
            .map(|rel| Ok((rel.clone(), hash(&self.read(rel)?))))
            .collect()
    }

    /// Mirror this tree into `store`: digest every local file, diff it against
    /// what the store holds, then upload and delete what the [`Plan`] calls for,
    /// one reported line per file. A dry run stops after the preview.
    ///
    /// THE reconcile loop: every backend runs this one, so a deploy reports the
    /// same lines and honours `--dry-run` and `delete` the same way whichever
    /// destination it targets.
    pub fn reconcile(
        &self,
        store: &dyn Store,
        delete: bool,
        opts: &Options,
        ui: &Ui,
    ) -> Result<()> {
        let local = self.digests(|bytes| store.digest(bytes))?;
        let plan = Plan::compute(&local, &store.list()?, delete);
        plan.preview(ui, opts.dry_run);
        if opts.dry_run {
            return Ok(());
        }
        for key in &plan.uploads {
            store.upload(key, &self.read(key)?)?;
            ui.item(format_args!("{} {key}", Marker::Uploaded));
        }
        for key in &plan.deletes {
            store.delete(key)?;
            ui.item(format_args!("{} {key}", Marker::Removed));
        }
        plan.done(ui, store.target());
        Ok(())
    }
}

/// The remote side of a deploy: the store a [`Dist`] is mirrored into. Only the
/// wire differs between destinations (S3 over signed HTTP, SSH over SFTP), so a
/// backend supplies these operations and [`Dist::reconcile`] drives them.
pub trait Store {
    /// Digest a local file's bytes with the same algorithm [`Store::list`]
    /// reports, so the two are comparable: S3 compares MD5 ETags, SSH the
    /// host's SHA-256.
    fn digest(&self, bytes: &[u8]) -> String;

    /// Everything the store currently holds, keyed by dist-relative path.
    fn list(&self) -> Result<Digests>;

    /// Write `body` at the dist-relative `key`.
    fn upload(&self, key: &str, body: &[u8]) -> Result<()>;

    /// Remove the dist-relative `key`.
    fn delete(&self, key: &str) -> Result<()>;

    /// The destination as the summary line names it.
    fn target(&self) -> String;
}

/// Deploy to every configured destination in turn. Errors if none is configured,
/// so `baudelaire deploy` on an unconfigured project explains itself rather than
/// silently doing nothing.
pub fn run(config: &Config, opts: &Options, ui: &Ui) -> Result<()> {
    let backends = configured(config);
    // `deploy` never constructs an `Engine`, so the gate table that warns the
    // build-shaped commands about a missing capability never runs here. Say it
    // at the one place that can: as a warning when another destination still
    // carries the run, and as an error when skipping ssh leaves nothing to do
    // (which `Unconfigured` would otherwise misreport as an empty config).
    #[cfg(not(feature = "ssh"))]
    if config.deploy.ssh.is_some() {
        if backends.is_empty() {
            return Err(DeployError::SshUnsupported.into());
        }
        ui.warn(crate::error::warning::FeatureMissing {
            setting: "deploy { ssh }",
            cargo: "ssh",
            effect: "the SSH destination is skipped",
        });
    }
    if backends.is_empty() {
        return Err(DeployError::Unconfigured.into());
    }
    let dist = Dist::scan(&config.paths.dist)?;
    remote::publish(
        "deploy",
        backends,
        &dist,
        |dist| Count::files(dist.files.len()).to_string(),
        opts,
        ui,
    )
}

/// The enabled destinations, from config alone. THE single source of what a
/// `deploy` run targets: add a backend by adding one line here.
fn configured(config: &Config) -> Vec<Box<dyn Backend<Dist>>> {
    let mut out: Vec<Box<dyn Backend<Dist>>> = Vec::new();
    if let Some(s3) = &config.deploy.s3 {
        out.push(Box::new(S3::new(s3.clone())));
    }
    #[cfg(feature = "ssh")]
    if let Some(ssh) = &config.deploy.ssh {
        out.push(Box::new(Ssh::new(ssh.clone())));
    }
    out
}

/// What a reconcile will do to the remote, shared by every [`Backend`], which
/// hashes its files, lists the remote, and diffs the two through [`Plan::compute`].
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Plan {
    /// Keys to upload (new or changed).
    pub uploads: Vec<String>,
    /// Keys to delete (present remotely, gone locally).
    pub deletes: Vec<String>,
    /// How many files are already up to date.
    pub unchanged: usize,
}

impl Plan {
    /// Diff local digests against remote digests, both keyed by dist-relative
    /// path. A file uploads when its digest differs from the remote's (or it is
    /// new); an entry deletes when the build no longer produces it and `delete`
    /// is on; everything else is unchanged. The digest algorithm is the backend's
    /// choice; this only compares the strings.
    pub fn compute(local: &Digests, remote: &Digests, delete: bool) -> Plan {
        let mut out = Plan::default();
        for (key, digest) in local {
            match remote.get(key) {
                Some(other) if other.eq_ignore_ascii_case(digest) => out.unchanged += 1,
                _ => out.uploads.push(key.clone()),
            }
        }
        if delete {
            out.deletes = remote
                .keys()
                .filter(|key| !local.contains_key(*key))
                .cloned()
                .collect();
        }
        out
    }

    /// Announce the plan: a one-line summary, prefixed on a dry run so the preview
    /// reads as a preview.
    fn preview(&self, ui: &Ui, dry_run: bool) {
        let lead = if dry_run {
            "dry run: would deploy "
        } else {
            ""
        };
        ui.detail(format_args!(
            "{lead}{} to upload, {} to delete, {} unchanged",
            Count::files(self.uploads.len()),
            Count::files(self.deletes.len()),
            self.unchanged
        ));
    }

    /// Announce the completed reconcile against `target`.
    fn done(&self, ui: &Ui, target: impl std::fmt::Display) {
        ui.done(format_args!(
            "deployed to {target}: {} uploaded, {} deleted",
            self.uploads.len(),
            self.deletes.len()
        ));
    }
}

#[cfg(test)]
mod tests {
    /// Remote-supplied paths that would escape the deploy root are refused: a
    /// hostile host answering with one used to have the client delete a file
    /// outside the project.
    #[test]
    fn listed_refuses_paths_escaping_the_root() {
        for path in [
            "../etc/passwd",
            "a/../../etc/passwd",
            "/etc/nginx/sites-enabled/x",
            "",
            "a//b",
            "a/./b",
            "..",
            r"a\..\..\windows",
        ] {
            assert!(super::Listed::try_from(path).is_err(), "accepted {path:?}");
        }
    }

    #[test]
    fn listed_accepts_ordinary_relative_paths() {
        for path in ["index.html", "posts/a/index.html", "assets/app.abc123.css"] {
            assert!(super::Listed::try_from(path).is_ok(), "refused {path:?}");
        }
    }

    use super::*;

    #[test]
    fn unconfigured_deploy_errors() {
        let config = Config::default();
        assert!(configured(&config).is_empty());
        let opts = Options {
            dry_run: true,
            yes: true,
            secret: None,
            interaction: &Headless,
        };
        assert!(matches!(
            run(&config, &opts, &Ui::new(Level::Silent)),
            Err(crate::error::BaudelaireErrorKind::Deploy(
                DeployError::Unconfigured
            ))
        ));
    }

    struct Headless;
    impl crate::remote::Interaction for Headless {
        fn confirm(&self, _: &str) -> Result<bool> {
            Ok(true)
        }
        fn secret(&self, _: &str) -> Result<Option<String>> {
            Ok(None)
        }
    }

    fn digests(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn plan_uploads_new_and_changed_skips_matching() {
        let local = digests(&[
            ("new.html", "aa"),
            ("same.html", "bb"),
            ("changed.html", "cc"),
        ]);
        let remote = digests(&[("same.html", "bb"), ("changed.html", "old")]);
        let plan = Plan::compute(&local, &remote, true);
        assert_eq!(
            plan.uploads,
            vec!["changed.html".to_string(), "new.html".to_string()]
        );
        assert_eq!(plan.unchanged, 1);
    }

    #[test]
    fn plan_deletes_orphans_only_when_enabled() {
        let local = digests(&[("keep.html", "k")]);
        let remote = digests(&[("keep.html", "k"), ("gone.html", "g")]);

        let with_delete = Plan::compute(&local, &remote, true);
        assert_eq!(with_delete.deletes, vec!["gone.html".to_string()]);
        assert_eq!(with_delete.unchanged, 1);

        assert!(Plan::compute(&local, &remote, false).deletes.is_empty());
    }

    #[test]
    fn plan_compares_digests_case_insensitively() {
        let local = digests(&[("a.html", "abcdef")]);
        let remote = digests(&[("a.html", "ABCDEF")]);
        assert_eq!(Plan::compute(&local, &remote, true).unchanged, 1);
    }
}
