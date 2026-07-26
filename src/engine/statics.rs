//! Static passthrough: files copied verbatim from `config.static` into the
//! `dist` root: no minify, no bundle, no fingerprint, no URL prefix. The
//! escape hatch for anything the asset pipeline would otherwise rewrite: a
//! `robots.txt` override, `.well-known/`, a `CNAME`, an `install.sh`.
//!
//! Runs before the asset pipeline and page writes, so a generated file at the
//! same output path wins; static is the lowest-priority source.

use std::path::{Path, PathBuf};

use crate::config::Config;
use crate::error::Result;
use crate::fs;

/// Mirrors the static tree into `dist`, preserving its layout at the site root.
pub struct Static<'a> {
    src: &'a Path,
    dist: &'a Path,
    /// The served asset directory and the tree the pipeline stages it in. A
    /// static file landing inside the asset directory is written to the staging
    /// tree, so [`crate::engine::asset::Assets::publish`] carries it into place
    /// instead of deleting it with the directory it replaces.
    assets: (PathBuf, PathBuf),
}

/// The outcome of a static copy: files written this build and their byte size.
/// Files skipped as already-current do not count; an unchanged tree reports 0.
/// `paths` lists every destination the static tree owns (copied or skipped), so
/// the prune pass keeps them.
#[derive(Default)]
pub struct Copied {
    pub count: usize,
    pub bytes: u64,
    pub paths: Vec<PathBuf>,
}

impl<'a> Static<'a> {
    pub fn new(config: &'a Config) -> Self {
        Self {
            src: &config.r#static,
            dist: &config.dist,
            assets: (config.asset_dist(), config.asset_staging()),
        }
    }

    /// Copy every file under `src` to the same relative path under `dist`,
    /// skipping any already [`current`](Static::current), so an unchanged tree
    /// costs no writes on rebuild and the dev server's live-reload isn't churned
    /// by untouched files. A missing `src` is not an error: the directory is
    /// optional.
    pub fn copy(&self) -> Result<Copied> {
        let mut out = Copied::default();
        if !self.src.exists() {
            return Ok(out);
        }
        for file in fs::Walk::new(self.src).files()? {
            let rel = file
                .strip_prefix(self.src)
                .expect("Walk yields paths under src");
            let len = file.metadata().map(|m| m.len()).unwrap_or(0);
            // The prune keeps the *served* path; the write may go elsewhere.
            out.paths.push(self.dist.join(rel));
            let dst = self.destination(rel);
            if Self::current(&file, &dst) {
                continue;
            }
            if let Some(parent) = dst.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(&file, &dst)?;
            Self::stamp(&file, &dst);
            out.count += 1;
            out.bytes += len;
        }
        Ok(out)
    }

    /// Where a static file at `rel` is written: its place under `dist`, unless
    /// that falls inside the asset directory, which the pipeline replaces
    /// wholesale — those go to the staging tree and are published with it.
    fn destination(&self, rel: &Path) -> PathBuf {
        let (served, staging) = &self.assets;
        let direct = self.dist.join(rel);
        match direct.strip_prefix(served) {
            Ok(inside) => staging.join(inside),
            Err(_) => direct,
        }
    }

    /// Whether `dst` already holds `src` verbatim: same size and the *same*
    /// mtime, which [`Static::stamp`] gave it when it was copied.
    ///
    /// The old rule was `dst` no *older* than `src`, which a same-size edit
    /// defeats: a `git checkout` or `stash`, an archive extraction or `touch -d`
    /// leaves the destination newer, and the file is never copied again.
    /// Equality instead, against a timestamp this pass controls, so an edit
    /// always shows up while an unchanged tree still costs two `stat`s per file
    /// rather than a full read of both. Best-effort; any error means copy.
    fn current(src: &Path, dst: &Path) -> bool {
        let (Ok(s), Ok(d)) = (src.metadata(), dst.metadata()) else {
            return false;
        };
        s.len() == d.len() && matches!((s.modified(), d.modified()), (Ok(sm), Ok(dm)) if sm == dm)
    }

    /// Give `dst` the source's mtime, so [`Static::current`] can compare the two
    /// directly. `fs::copy` copies permissions but not timestamps, so without
    /// this every destination looks newer than its source and the cheap check
    /// could never say "unchanged".
    fn stamp(src: &Path, dst: &Path) {
        let Ok(modified) = src.metadata().and_then(|meta| meta.modified()) else {
            return;
        };
        // Best-effort: a filesystem that refuses the timestamp costs a re-copy
        // next build, nothing worse.
        if let Ok(file) = std::fs::File::options().write(true).open(dst) {
            let _ = file.set_modified(modified);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Static;
    use std::fs;
    use std::time::Duration;

    /// A same-size edit whose destination is *newer* than the source used to be
    /// skipped forever: exactly what `git checkout` between two branches
    /// produces.
    #[test]
    fn a_same_size_edit_is_copied_even_when_the_destination_is_newer() {
        let tmp = tempfile::tempdir().unwrap();
        let (src, dst) = (tmp.path().join("CNAME"), tmp.path().join("out/CNAME"));
        fs::create_dir_all(dst.parent().unwrap()).unwrap();
        fs::write(&src, b"a.example.com").unwrap();
        fs::write(&dst, b"b.example.com").unwrap();
        // Whatever the clock did, the destination is at least as new.
        let newer = fs::metadata(&src).unwrap().modified().unwrap() + Duration::from_secs(60);
        fs::File::options()
            .write(true)
            .open(&dst)
            .unwrap()
            .set_modified(newer)
            .unwrap();

        assert!(!Static::current(&src, &dst));
    }

    /// A copied file is stamped with its source's mtime, so the next build skips
    /// it on two `stat`s rather than reading both files in full.
    #[test]
    fn a_stamped_copy_reads_as_current() {
        let tmp = tempfile::tempdir().unwrap();
        let (src, dst) = (tmp.path().join("CNAME"), tmp.path().join("out/CNAME"));
        fs::create_dir_all(dst.parent().unwrap()).unwrap();
        fs::write(&src, b"a.example.com").unwrap();
        fs::copy(&src, &dst).unwrap();

        Static::stamp(&src, &dst);
        assert!(Static::current(&src, &dst));

        // ...and an edit of the same length still shows up.
        fs::write(&src, b"b.example.com").unwrap();
        assert!(!Static::current(&src, &dst));
    }

    #[test]
    fn a_missing_destination_is_never_current() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("robots.txt");
        fs::write(&src, b"x").unwrap();
        assert!(!Static::current(&src, &tmp.path().join("absent.txt")));
    }
}
