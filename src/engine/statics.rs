//! Static passthrough: files copied verbatim from `config.static` into the
//! `dist` root — no minify, no bundle, no fingerprint, no URL prefix. The
//! escape hatch for anything the asset pipeline would otherwise rewrite: a
//! `robots.txt` override, `.well-known/`, a `CNAME`, an `install.sh`.
//!
//! Runs before the asset pipeline and page writes, so a generated file at the
//! same output path wins — static is the lowest-priority source.

use std::path::Path;

use crate::config::Config;
use crate::engine::asset::Walk;
use crate::error::Result;
use crate::fs;

/// Mirrors the static tree into `dist`, preserving its layout at the site root.
pub struct Static<'a> {
    src: &'a Path,
    dist: &'a Path,
}

/// The outcome of a static copy: files written this build and their byte size.
/// Files skipped as already-current do not count — an unchanged tree reports 0.
#[derive(Default)]
pub struct Copied {
    pub count: usize,
    pub bytes: u64,
}

impl<'a> Static<'a> {
    pub fn new(config: &'a Config) -> Self {
        Self {
            src: &config.r#static,
            dist: &config.dist,
        }
    }

    /// Copy every file under `src` to the same relative path under `dist`,
    /// skipping any already present with an identical size and no older than
    /// its source — so an unchanged tree costs no writes on rebuild, and the
    /// dev server's live-reload isn't churned by untouched files. A missing
    /// `src` is not an error: the directory is optional.
    pub fn copy(&self) -> Result<Copied> {
        let mut out = Copied::default();
        if !self.src.exists() {
            return Ok(out);
        }
        for file in Walk::files(self.src)? {
            let rel = file
                .strip_prefix(self.src)
                .expect("Walk yields paths under src");
            let dst = self.dist.join(rel);
            let len = file.metadata().map(|m| m.len()).unwrap_or(0);
            if Self::current(&file, &dst) {
                continue;
            }
            if let Some(parent) = dst.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(&file, &dst)?;
            out.count += 1;
            out.bytes += len;
        }
        Ok(out)
    }

    /// Whether `dst` already holds `src` verbatim: same size and last modified
    /// no earlier than the source. Best-effort — any metadata error means copy.
    fn current(src: &Path, dst: &Path) -> bool {
        let (Ok(s), Ok(d)) = (src.metadata(), dst.metadata()) else {
            return false;
        };
        s.len() == d.len() && matches!((s.modified(), d.modified()), (Ok(sm), Ok(dm)) if dm >= sm)
    }
}
