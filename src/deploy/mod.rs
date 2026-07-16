//! Deploying the built site's *files* to a host — the counterpart to
//! [`crate::announce`], which publishes metadata records. A destination
//! implements one [`Backend`]; it receives the built [`Dist`] and reconciles the
//! remote with it (upload changed, delete removed). Adding a destination is one
//! `impl Backend` plus one line in [`configured`]; nothing else learns about it.

mod s3;
mod sigv4;

use std::path::{Path, PathBuf};

use crate::config::Config;
use crate::error::{DeployError, Result};
use crate::remote::Options;
use crate::ui::{Count, Ui};

#[cfg(test)]
use crate::ui::Level;

use self::s3::S3;

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
        collect(root, root, &mut files)?;
        files.sort();
        Ok(Self { root: root.to_owned(), files })
    }

    /// Read one file's bytes by its relative path.
    pub fn read(&self, rel: &str) -> Result<Vec<u8>> {
        crate::fs::read(self.root.join(rel))
    }
}

/// Append every file under `dir` to `out`, as a path relative to `base` with
/// forward slashes — recursing into subdirectories.
fn collect(base: &Path, dir: &Path, out: &mut Vec<String>) -> Result<()> {
    for path in crate::fs::read_dir(dir)? {
        if path.is_dir() {
            collect(base, &path, out)?;
        } else if let Ok(rel) = path.strip_prefix(base) {
            out.push(rel.to_string_lossy().replace('\\', "/"));
        }
    }
    Ok(())
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
    out
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
}
