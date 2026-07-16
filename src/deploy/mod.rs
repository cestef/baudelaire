//! Deploying the built site's *files* to a host — the counterpart to
//! [`crate::announce`], which publishes metadata records. A destination
//! implements one [`Backend`]; it receives the built [`Dist`] and reconciles the
//! remote with it (upload changed, delete removed). Adding a destination is one
//! `impl Backend` plus one line in [`configured`]; nothing else learns about it.

mod s3;
mod sigv4;
mod ssh;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::config::Config;
use crate::error::{DeployError, Result};
use crate::remote::Options;
use crate::ui::{Count, Ui};

/// Files keyed by dist-relative path, each mapped to a content digest. The
/// currency every [`Backend`] reconciles in — local digests versus remote.
pub type Digests = BTreeMap<String, String>;

#[cfg(test)]
use crate::ui::Level;

use self::s3::S3;
use self::ssh::Ssh;

/// The built output tree handed to every [`Backend`]: the `dist` root and every
/// file under it as a forward-slashed, root-relative path. Bytes are read on
/// demand rather than held, so reconciling a large site stays streaming — a
/// backend hashes each file to decide what changed, then reads only what it
/// uploads.
pub struct Dist {
    root: PathBuf,
    /// Every file under `root`, as a sorted, forward-slashed relative path.
    pub files: Vec<String>,
}

impl Dist {
    /// Walk `root` into the set of relative file paths, following subdirectories.
    fn scan(root: &Path) -> Result<Self> {
        let mut files = Vec::new();
        Self::walk(root, root, &mut files)?;
        files.sort();
        Ok(Self { root: root.to_owned(), files })
    }

    /// Append every file under `dir` to `out`, as a path relative to `base` with
    /// forward slashes — recursing into subdirectories.
    fn walk(base: &Path, dir: &Path, out: &mut Vec<String>) -> Result<()> {
        for path in crate::fs::read_dir(dir)? {
            if path.is_dir() {
                Self::walk(base, &path, out)?;
            } else if let Ok(rel) = path.strip_prefix(base) {
                out.push(rel.to_string_lossy().replace('\\', "/"));
            }
        }
        Ok(())
    }

    /// Read one file's bytes by its relative path.
    pub fn read(&self, rel: &str) -> Result<Vec<u8>> {
        crate::fs::read(self.root.join(rel))
    }

    /// Digest every file with `hash`, keyed by relative path. Each file is read
    /// once, hashed, and dropped, so a large site never sits wholly in memory;
    /// the algorithm is the backend's choice (S3 wants MD5, SSH SHA-256).
    pub fn digests(&self, hash: impl Fn(&[u8]) -> String) -> Result<Digests> {
        self.files.iter().map(|rel| Ok((rel.clone(), hash(&self.read(rel)?)))).collect()
    }
}

/// A file destination the built site can be deployed to.
pub trait Backend {
    /// Stable, human-facing name, shown in progress output.
    fn name(&self) -> &'static str;

    /// Reconcile `dist` with this destination under `opts`, reporting progress.
    /// Honors `opts.dry_run` by computing and reporting the plan without writing.
    fn run(&self, dist: &Dist, opts: &Options, ui: &Ui) -> Result<()>;
}

/// Deploy to every configured destination in turn. Errors if none is configured,
/// so `baudelaire deploy` on an unconfigured project explains itself rather than
/// silently doing nothing.
pub fn run(config: &Config, opts: &Options, ui: &Ui) -> Result<()> {
    let backends = configured(config);
    if backends.is_empty() {
        return Err(DeployError::Unconfigured.into());
    }
    let dist = Dist::scan(&config.dist)?;
    for backend in backends {
        ui.section(format_args!("{} — {}", backend.name(), Count::files(dist.files.len())));
        // Confirm before any network mutation, unless previewing or `--yes`.
        if !opts.dry_run && !opts.confirm(&format!("deploy to {}?", backend.name()))? {
            ui.detail(format_args!("skipped {}", backend.name()));
            continue;
        }
        backend.run(&dist, opts, ui)?;
    }
    Ok(())
}

/// The enabled destinations, from config alone. THE single source of what a
/// `deploy` run targets: add a backend by adding one line here.
fn configured(config: &Config) -> Vec<Box<dyn Backend>> {
    let mut out: Vec<Box<dyn Backend>> = Vec::new();
    if let Some(s3) = &config.deploy.s3 {
        out.push(Box::new(S3::new(s3.clone())));
    }
    if let Some(ssh) = &config.deploy.ssh {
        out.push(Box::new(Ssh::new(ssh.clone())));
    }
    out
}

/// What a reconcile will do to the remote — shared by every [`Backend`], which
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
    /// choice — this only compares the strings.
    pub fn compute(local: &Digests, remote: &Digests, delete: bool) -> Plan {
        let mut out = Plan::default();
        for (key, digest) in local {
            match remote.get(key) {
                Some(other) if other.eq_ignore_ascii_case(digest) => out.unchanged += 1,
                _ => out.uploads.push(key.clone()),
            }
        }
        if delete {
            out.deletes = remote.keys().filter(|key| !local.contains_key(*key)).cloned().collect();
        }
        out
    }

    /// Announce the plan: a one-line summary, prefixed on a dry run so the preview
    /// reads as a preview.
    fn preview(&self, ui: &Ui, dry_run: bool) {
        let lead = if dry_run { "dry run — would deploy " } else { "" };
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
            "deployed to {target} — {} uploaded, {} deleted",
            self.uploads.len(),
            self.deletes.len()
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unconfigured_deploy_errors() {
        let config = Config::default();
        assert!(configured(&config).is_empty());
        let opts = Options { dry_run: true, yes: true, secret: None, interaction: &Headless };
        assert!(matches!(
            run(&config, &opts, &Ui::new(Level::Silent)),
            Err(crate::error::BaudelaireErrorKind::Deploy(DeployError::Unconfigured))
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
        pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
    }

    #[test]
    fn plan_uploads_new_and_changed_skips_matching() {
        let local = digests(&[("new.html", "aa"), ("same.html", "bb"), ("changed.html", "cc")]);
        let remote = digests(&[("same.html", "bb"), ("changed.html", "old")]);
        let plan = Plan::compute(&local, &remote, true);
        assert_eq!(plan.uploads, vec!["changed.html".to_string(), "new.html".to_string()]);
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
