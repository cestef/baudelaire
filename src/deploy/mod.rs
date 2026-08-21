//! Deploying the built site's files to a host: a destination implements one
//! [`Backend`] and supplies the [`Store`] that [`Dist::reconcile`] mirrors into.

mod digest;
mod s3;
mod sigv4;
#[cfg(feature = "ssh")]
mod ssh;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::config::{Config, Slashed};
use crate::engine::gate::Gate;
use crate::error::deploy::Phase;
use crate::error::warning::RemotePathsRefused;
use crate::error::{BaudelaireErrorKind, DeployError, Result};
use crate::remote::{Backend, Options};
use crate::ui::{Count, Marker, Ui};

/// Files keyed by dist-relative path, each mapped to a content digest.
pub type Digests = BTreeMap<String, String>;

/// A path the *remote* named, admitted only if it stays inside the deploy root.
///
/// A remote-supplied path feeds straight into a delete, so only
/// [`Listed::try_from`] constructs one and every function that turns such a
/// path into a request takes one of these rather than a `&str`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Listed(String);

impl TryFrom<&str> for Listed {
    type Error = ();

    /// Accepts a plain relative path confined to the root: not absolute, no
    /// `..`, no empty or bare-`.` component, no backslash (which a remote could
    /// use as a separator this does not split on).
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
    pub fn into_string(self) -> String {
        self.0
    }
}

/// A remote's own file list, split into what this client may act on and what
/// [`Listed`] refused.
///
/// The refusals are carried rather than dropped: a key missing from the listing
/// is one the reconcile can neither upload over nor delete, and nothing in the
/// run would ever say it is there.
#[derive(Debug, Default)]
pub struct Inventory {
    files: Digests,
    refused: Vec<(String, &'static str)>,
}

impl Inventory {
    /// Why a listed path at a destination the reconcile does not own is refused.
    pub(crate) const OUTSIDE: &'static str = "outside the deploy root";

    fn admit(&mut self, path: String, digest: String) {
        self.files.insert(path, digest);
    }

    /// Record a listed path the reconcile may not act on, with the reason the
    /// report gives for it.
    fn refuse(&mut self, path: impl Into<String>, why: &'static str) {
        self.refused.push((path.into(), why));
    }

    /// Report what was refused, and hand back what the reconcile may act on.
    fn report(self, ui: &Ui, target: &str) -> Digests {
        if !self.refused.is_empty() {
            for (path, why) in &self.refused {
                ui.skip(path, why);
            }
            ui.warn(RemotePathsRefused {
                count: self.refused.len(),
                target: target.to_owned(),
            });
        }
        self.files
    }
}

#[cfg(test)]
use crate::ui::Level;

use self::s3::{Fingerprinted, S3};
#[cfg(feature = "ssh")]
use self::ssh::Ssh;

/// The built output tree handed to every [`Backend`]: the `dist` root and every
/// file under it as a forward-slashed, root-relative path.
pub struct Dist {
    root: PathBuf,
    /// Every file under `root`, as a sorted, forward-slashed relative path.
    pub files: Vec<String>,
}

impl Dist {
    /// Walk `root` into the set of relative file paths.
    ///
    /// A walk stays inside the tree it was rooted at, so a symlink pointing
    /// elsewhere cannot have the deploy upload files the build never produced.
    fn scan(root: &Path) -> Result<Self> {
        let mut files: Vec<String> = crate::fs::Walk::new(root)
            .files()?
            .iter()
            .filter_map(|path| path.strip_prefix(root).ok())
            .map(|rel| Slashed(rel).to_string())
            .collect();
        files.sort();
        Ok(Self {
            root: root.to_owned(),
            files,
        })
    }

    pub fn read(&self, rel: &str) -> Result<Vec<u8>> {
        crate::fs::read(self.root.join(rel))
    }

    /// Digest every file with `hash`, keyed by relative path.
    ///
    /// Reading and hashing are the local half of a deploy and answer to nothing
    /// remote, so they run over the build's own threads whatever the store
    /// takes.
    pub fn digests(&self, hash: impl Fn(&[u8]) -> String + Sync) -> Result<Digests> {
        use rayon::prelude::*;
        self.files
            .par_iter()
            .map(|rel| Ok((rel.clone(), hash(&self.read(rel)?))))
            .collect()
    }

    /// Mirror this tree into `store`: digest every local file, diff it against
    /// what the store holds, then upload and delete what the [`Plan`] calls
    /// for. A dry run stops after the preview.
    pub fn reconcile(
        &self,
        store: &dyn Store,
        delete: bool,
        opts: &Options,
        ui: &Ui,
    ) -> Result<()> {
        let local = self.digests(|bytes| store.digest(bytes))?;
        let plan = Plan::compute(&local, &store.list(ui)?, delete);
        plan.preview(ui, opts.dry_run);
        if opts.dry_run {
            return Ok(());
        }
        let concurrency = store.concurrency();
        Self::each(
            &plan.uploads,
            Phase::Upload,
            Marker::Uploaded,
            concurrency,
            ui,
            |key| self.read(key).and_then(|body| store.upload(key, &body)),
        )?;
        Self::each(
            &plan.deletes,
            Phase::Delete,
            Marker::Removed,
            concurrency,
            ui,
            |key| store.delete(key),
        )?;
        plan.done(ui, store.target());
        Ok(())
    }

    /// Act on every key of one phase, in as many threads as the store takes,
    /// and stop the run at the first failure.
    ///
    /// A concurrent run names *a* key that failed rather than the first in
    /// order, since the requests are in flight together; the count beside it is
    /// how many had finished when the run stopped.
    fn each(
        keys: &[String],
        phase: Phase,
        marker: Marker,
        concurrency: Option<usize>,
        ui: &Ui,
        act: impl Fn(&str) -> Result<()> + Sync,
    ) -> Result<()> {
        let done = std::sync::atomic::AtomicUsize::new(0);
        let failed: parking_lot::Mutex<Option<(String, BaudelaireErrorKind)>> =
            parking_lot::Mutex::new(None);
        let one = |key: &String| -> std::result::Result<(), Stop> {
            match act(key) {
                Ok(()) => {
                    done.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    ui.item(format_args!("{marker} {key}"));
                    Ok(())
                }
                Err(why) => {
                    failed.lock().get_or_insert((key.clone(), why));
                    Err(Stop)
                }
            }
        };
        let ran = match concurrency {
            Some(1) => keys.iter().try_for_each(one),
            threads => Pool::of(threads).install(|| {
                use rayon::prelude::*;
                keys.par_iter().try_for_each(one)
            }),
        };
        if ran.is_ok() {
            return Ok(());
        }
        let (key, why) = failed.lock().take().expect("a stopped run failed");
        Err(DeployError::interrupted(
            phase,
            done.load(std::sync::atomic::Ordering::Relaxed),
            keys.len(),
            &key,
            why,
        )
        .into())
    }
}

/// That a key failed, which is all the loop carries: the failure itself is put
/// aside, since a `Result` big enough to hold one would be paid for on every
/// key that did not.
struct Stop;

/// The threads one phase of a reconcile runs over: a pool of the store's own
/// size, or the build's own threads where it names none.
enum Pool {
    Own(rayon::ThreadPool),
    Build,
}

impl Pool {
    fn of(threads: Option<usize>) -> Self {
        let Some(threads) = threads else {
            return Self::Build;
        };
        match rayon::ThreadPoolBuilder::new().num_threads(threads).build() {
            Ok(pool) => Self::Own(pool),
            Err(e) => {
                tracing::debug!("deploy pool of {threads} not built: {e}");
                Self::Build
            }
        }
    }

    fn install<T: Send>(&self, run: impl FnOnce() -> T + Send) -> T {
        match self {
            Self::Own(pool) => pool.install(run),
            Self::Build => run(),
        }
    }
}

/// The remote side of a deploy: the store a [`Dist`] is mirrored into.
pub trait Store: Sync {
    /// Digest a local file's bytes with the same algorithm [`Store::list`]
    /// reports, so the two are comparable.
    fn digest(&self, bytes: &[u8]) -> String;

    /// Everything the store currently holds, keyed by dist-relative path.
    ///
    /// Gathers its answer into an [`Inventory`] and reports through it, since a
    /// listing is where a remote names paths this client will not act on.
    fn list(&self, ui: &Ui) -> Result<Digests>;

    /// Write `body` at the dist-relative `key`.
    fn upload(&self, key: &str, body: &[u8]) -> Result<()>;

    /// Remove the dist-relative `key`.
    fn delete(&self, key: &str) -> Result<()>;

    /// The destination as the summary line names it.
    fn target(&self) -> String;

    /// How many requests this store answers at once: `Some(1)` for a store
    /// driven over one connection, `None` for as many as the build has threads.
    fn concurrency(&self) -> Option<usize>;
}

/// The `deploy` command: which destinations a run targets, and what it hands
/// them.
pub struct Deploy;

impl Deploy {
    /// Deploy to every configured destination in turn.
    ///
    /// `deploy` never constructs an `Engine`, so a missing `ssh` capability is
    /// reported here rather than by the gate table the build-shaped commands
    /// run through.
    pub fn run(config: &Config, opts: &Options, ui: &Ui) -> Result<()> {
        let backends = Self::configured(config)?;
        if config.deploy.ssh.is_some()
            && let Some(gap) = Gate::missing_for(Gate::SSH)
        {
            #[cfg(not(feature = "ssh"))]
            if backends.is_empty() {
                return Err(DeployError::SshUnsupported.into());
            }
            ui.warn(gap);
        }
        if backends.is_empty() {
            return Err(DeployError::Unconfigured.into());
        }
        let dist = Dist::scan(&config.paths.dist)?;
        opts.publish(
            "deploy",
            backends,
            &dist,
            |dist| Count::files(dist.files.len()).to_string(),
            ui,
        )
    }

    /// The enabled destinations, from config alone: add a backend by adding one
    /// line here.
    ///
    /// Each destination checks its own block before it is built, so a setting
    /// it cannot work without is refused here rather than turning into a
    /// request against the wrong place.
    fn configured(config: &Config) -> Result<Vec<Box<dyn Backend<Dist>>>> {
        let mut out: Vec<Box<dyn Backend<Dist>>> = Vec::new();
        if let Some(s3) = &config.deploy.s3 {
            S3::check(s3)?;
            out.push(Box::new(S3::new(
                s3.clone(),
                config.headers.cache.clone(),
                Fingerprinted {
                    prefix: config.asset_name().to_owned(),
                    hashed: config.assets.fingerprint,
                },
            )));
        }
        #[cfg(feature = "ssh")]
        if let Some(ssh) = &config.deploy.ssh {
            Ssh::check(ssh)?;
            out.push(Box::new(Ssh::new(ssh.clone())));
        }
        Ok(out)
    }
}

/// What a reconcile will do to the remote.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Plan {
    pub uploads: Vec<String>,
    pub deletes: Vec<String>,
    pub unchanged: usize,
}

impl Plan {
    /// Diff local digests against remote digests, both keyed by dist-relative
    /// path and both in the backend's own digest algorithm.
    pub fn compute(local: &Digests, remote: &Digests, delete: bool) -> Self {
        let mut out = Self::default();
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

    /// Announce the plan as a one-line summary, prefixed on a dry run.
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

    /// A store that records what it was asked to do, and how many requests were
    /// in flight at the busiest moment.
    struct Fake {
        concurrency: Option<usize>,
        acted: parking_lot::Mutex<Vec<String>>,
        inflight: std::sync::atomic::AtomicUsize,
        peak: std::sync::atomic::AtomicUsize,
        /// The key every request fails on, if any.
        refusing: Option<&'static str>,
    }

    impl Fake {
        fn new(concurrency: Option<usize>) -> Self {
            Self {
                concurrency,
                acted: parking_lot::Mutex::new(Vec::new()),
                inflight: std::sync::atomic::AtomicUsize::new(0),
                peak: std::sync::atomic::AtomicUsize::new(0),
                refusing: None,
            }
        }

        fn refusing(key: &'static str) -> Self {
            Self {
                refusing: Some(key),
                ..Self::new(Some(1))
            }
        }

        fn act(&self, what: &str, key: &str) -> Result<()> {
            use std::sync::atomic::Ordering::Relaxed;
            let now = self.inflight.fetch_add(1, Relaxed) + 1;
            self.peak.fetch_max(now, Relaxed);
            self.acted.lock().push(format!("{what} {key}"));
            self.inflight.fetch_sub(1, Relaxed);
            if self.refusing == Some(key) {
                return Err(DeployError::required(crate::error::deploy::Required::S3Bucket).into());
            }
            Ok(())
        }
    }

    impl Store for Fake {
        fn digest(&self, bytes: &[u8]) -> String {
            format!("{}", bytes.len())
        }

        fn list(&self, _ui: &Ui) -> Result<Digests> {
            Ok(Digests::from([("gone.html".to_owned(), "0".to_owned())]))
        }

        fn upload(&self, key: &str, _body: &[u8]) -> Result<()> {
            self.act("PUT", key)
        }

        fn delete(&self, key: &str) -> Result<()> {
            self.act("DELETE", key)
        }

        fn target(&self) -> String {
            "fake".to_owned()
        }

        fn concurrency(&self) -> Option<usize> {
            self.concurrency
        }
    }

    /// A dist of `n` files, named so their plan order is their name order.
    fn dist(dir: &std::path::Path, n: usize) -> Dist {
        for i in 0..n {
            std::fs::write(dir.join(format!("{i:03}.html")), format!("page {i}")).unwrap();
        }
        Dist::scan(dir).expect("a scannable dist")
    }

    fn opts() -> Options<'static> {
        Options {
            dry_run: false,
            yes: true,
            secret: None,
            interaction: &Headless,
        }
    }

    #[test]
    fn a_store_on_one_connection_acts_in_plan_order() {
        let tmp = tempfile::tempdir().unwrap();
        let dist = dist(tmp.path(), 4);
        let store = Fake::new(Some(1));

        dist.reconcile(&store, true, &opts(), &Ui::new(Level::Silent))
            .expect("mirrored");

        assert_eq!(
            *store.acted.lock(),
            vec![
                "PUT 000.html",
                "PUT 001.html",
                "PUT 002.html",
                "PUT 003.html",
                "DELETE gone.html",
            ]
        );
        assert_eq!(store.peak.load(std::sync::atomic::Ordering::Relaxed), 1);
    }

    #[test]
    fn a_store_that_takes_many_gets_every_key_once() {
        let tmp = tempfile::tempdir().unwrap();
        let dist = dist(tmp.path(), 32);
        let store = Fake::new(None);

        dist.reconcile(&store, true, &opts(), &Ui::new(Level::Silent))
            .expect("mirrored");

        let mut acted = store.acted.lock().clone();
        acted.sort();
        assert_eq!(acted.len(), 33, "32 uploads and one delete: {acted:?}");
        assert_eq!(
            acted.iter().filter(|line| line.starts_with("PUT")).count(),
            32
        );
        let mut keys: Vec<&String> = acted.iter().collect();
        keys.dedup();
        assert_eq!(keys.len(), acted.len(), "a key was acted on twice");
    }

    #[test]
    fn a_refused_key_stops_the_run_and_says_how_far_it_got() {
        let tmp = tempfile::tempdir().unwrap();
        let dist = dist(tmp.path(), 4);
        let store = Fake::refusing("002.html");

        let err = dist
            .reconcile(&store, true, &opts(), &Ui::new(Level::Silent))
            .expect_err("the store refused");

        let crate::error::BaudelaireErrorKind::Deploy(DeployError::Interrupted {
            phase,
            done,
            total,
            key,
            ..
        }) = err
        else {
            panic!("not an interrupted deploy: {err:?}");
        };
        assert!(matches!(phase, Phase::Upload), "not the upload phase");
        assert_eq!(key, "002.html");
        assert_eq!((done, total), (2, 4));
        assert!(
            !store
                .acted
                .lock()
                .iter()
                .any(|line| line.contains("DELETE")),
            "a failed upload phase must not go on to delete"
        );
    }

    #[test]
    fn unconfigured_deploy_errors() {
        let config = Config::default();
        let backends = Deploy::configured(&config).expect("nothing to check");
        assert!(backends.is_empty());
        let opts = Options {
            dry_run: true,
            yes: true,
            secret: None,
            interaction: &Headless,
        };
        assert!(matches!(
            Deploy::run(&config, &opts, &Ui::new(Level::Silent)),
            Err(crate::error::BaudelaireErrorKind::Deploy(
                DeployError::Unconfigured
            ))
        ));
    }

    #[test]
    fn an_incomplete_destination_is_refused_before_it_connects() {
        let refused = |text: &str| {
            matches!(
                Deploy::configured(&Config::parse(text).expect("parses")),
                Err(crate::error::BaudelaireErrorKind::Deploy(
                    DeployError::Required { .. } | DeployError::Relative { .. }
                ))
            )
        };
        assert!(refused("deploy { s3 { bucket \"\" } }"));
        assert!(!refused(
            "deploy { ssh { host \"srv\"; path \"/var/www\" } }"
        ));
        if cfg!(feature = "ssh") {
            assert!(refused("deploy { ssh { host \"srv\" } }"), "no path");
            assert!(refused("deploy { ssh { path \"/var/www\" } }"), "no host");
            assert!(
                refused("deploy { ssh { host \"srv\"; path \"www\" } }"),
                "a relative remote path"
            );
        }
    }

    #[test]
    fn a_refused_remote_path_is_reported_rather_than_dropped() {
        let mut inventory = Inventory::default();
        inventory.admit("index.html".to_owned(), "aa".to_owned());
        inventory.refuse("posts//a.html", Inventory::OUTSIDE);
        let ui = Ui::new(Level::Silent);
        let files = inventory.report(&ui, "bucket");
        assert_eq!(files.len(), 1);
        assert_eq!(ui.warnings(), 1);
    }

    struct Headless;
    impl crate::remote::Interaction for Headless {
        fn interactive(&self) -> bool {
            true
        }
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
            vec!["changed.html".to_owned(), "new.html".to_owned()]
        );
        assert_eq!(plan.unchanged, 1);
    }

    #[test]
    fn plan_deletes_orphans_only_when_enabled() {
        let local = digests(&[("keep.html", "k")]);
        let remote = digests(&[("keep.html", "k"), ("gone.html", "g")]);

        let with_delete = Plan::compute(&local, &remote, true);
        assert_eq!(with_delete.deletes, vec!["gone.html".to_owned()]);
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
