//! Static passthrough: files copied verbatim from `config.static` into the
//! `dist` root. Runs before the asset pipeline and page writes, so a generated
//! file at the same output path wins.

use std::path::{Path, PathBuf};

use crate::config::Config;
use crate::engine::layers::Layers;
use crate::error::Result;
use crate::fs;
use crate::theme::Theme;

/// Mirrors the static tree into `dist`, preserving its layout at the site root.
pub struct Static<'a> {
    /// Where static files are read from: the theme's tree beneath the project's,
    /// so a theme ships a `robots.txt` the site can replace.
    sources: Layers,
    dist: &'a Path,
    /// The served asset directory and the tree the pipeline stages it in: a
    /// static file landing inside the former is written to the latter, or
    /// [`crate::engine::asset::Assets::publish`] would delete it.
    assets: (PathBuf, PathBuf),
}

/// The outcome of a static copy: files written this build and their byte size,
/// so an unchanged tree reports 0. `paths` lists every destination the static
/// tree owns, copied or skipped, so the prune pass keeps them.
#[derive(Default)]
pub struct Copied {
    pub count: usize,
    pub bytes: u64,
    pub paths: Vec<PathBuf>,
}

impl<'a> Static<'a> {
    pub fn new(config: &'a Config, theme: Option<&Theme>) -> Self {
        Self {
            sources: Layers::new(theme.map(Theme::statics), &config.paths.r#static),
            dist: &config.paths.dist,
            assets: (config.asset_dist(), config.asset_staging()),
        }
    }

    /// Copy every file under `src` to the same relative path under `dist`,
    /// skipping any already [`current`](Static::current). A missing `src` is
    /// not an error: the directory is optional.
    pub fn copy(&self) -> Result<Copied> {
        let mut out = Copied::default();
        for source in self.sources.files()? {
            let file = &source.path;
            let rel = source.rel.as_path();
            let len = file.metadata().map_or(0, |m| m.len());
            out.paths.push(self.dist.join(rel));
            let dst = self.destination(rel);
            if Self::current(file, &dst) {
                continue;
            }
            if let Some(parent) = dst.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(file, &dst)?;
            Self::stamp(file, &dst);
            out.count += 1;
            out.bytes += len;
        }
        Ok(out)
    }

    /// Where a static file at `rel` is written: its place under `dist`, unless
    /// that falls inside the asset directory, which the pipeline replaces
    /// wholesale: those go to the staging tree and are published with it.
    fn destination(&self, rel: &Path) -> PathBuf {
        let (served, staging) = &self.assets;
        let direct = self.dist.join(rel);
        direct
            .strip_prefix(served)
            .map(|inside| staging.join(inside))
            .unwrap_or(direct)
    }

    /// Whether `dst` already holds `src` verbatim: same size and the *same*
    /// mtime, which [`Static::stamp`] gave it when it was copied. Equality, not
    /// "no older": a same-size edit can leave the destination newer. Any error
    /// means copy.
    fn current(src: &Path, dst: &Path) -> bool {
        let (Ok(s), Ok(d)) = (src.metadata(), dst.metadata()) else {
            return false;
        };
        s.len() == d.len() && matches!((s.modified(), d.modified()), (Ok(sm), Ok(dm)) if sm == dm)
    }

    /// Give `dst` the source's mtime, so [`Static::current`] can compare the two
    /// directly; `fs::copy` copies permissions but not timestamps. Best-effort:
    /// a filesystem that refuses it costs a re-copy next build.
    fn stamp(src: &Path, dst: &Path) {
        let Ok(modified) = src.metadata().and_then(|meta| meta.modified()) else {
            return;
        };
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

    #[test]
    fn a_same_size_edit_is_copied_even_when_the_destination_is_newer() {
        let tmp = tempfile::tempdir().unwrap();
        let (src, dst) = (tmp.path().join("CNAME"), tmp.path().join("out/CNAME"));
        fs::create_dir_all(dst.parent().unwrap()).unwrap();
        fs::write(&src, b"a.example.com").unwrap();
        fs::write(&dst, b"b.example.com").unwrap();
        let newer = fs::metadata(&src).unwrap().modified().unwrap() + Duration::from_mins(1);
        fs::File::options()
            .write(true)
            .open(&dst)
            .unwrap()
            .set_modified(newer)
            .unwrap();

        assert!(!Static::current(&src, &dst));
    }

    #[test]
    fn a_stamped_copy_reads_as_current() {
        let tmp = tempfile::tempdir().unwrap();
        let (src, dst) = (tmp.path().join("CNAME"), tmp.path().join("out/CNAME"));
        fs::create_dir_all(dst.parent().unwrap()).unwrap();
        fs::write(&src, b"a.example.com").unwrap();
        fs::copy(&src, &dst).unwrap();

        Static::stamp(&src, &dst);
        assert!(Static::current(&src, &dst));

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
