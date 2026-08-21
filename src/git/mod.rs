//! The site's git repository: what a build can learn from it.
//!
//! Every `git` process the build starts is started here, under one set of
//! options ([`run`]) and with every argument a constant of this module. Nothing
//! here can fail a build: a repository that cannot be read is a build with no
//! git state, which is exactly what a build outside one has.

mod format;
mod head;
mod log;
mod path;
mod run;

pub use head::Head;
pub use log::{Changed, History};

use std::path::{Path, PathBuf};

use run::Git;

/// What `git rev-parse --is-inside-work-tree` prints for a directory in one.
const INSIDE: &str = "true";

/// The repository a site sits in.
pub struct Repo {
    /// The site root. Every query runs here rather than at the repository root,
    /// so a site that is one directory of a larger repository asks about its
    /// own subtree.
    dir: PathBuf,
}

impl Repo {
    /// The repository `root` sits in, or `None` where there is none, where the
    /// work tree is bare, and where `git` is not installed.
    pub fn discover(root: &Path) -> Option<Self> {
        let dir = root.to_path_buf();
        let inside = Git::at(&dir).ask(&["rev-parse", "--is-inside-work-tree"])?;
        (inside == INSIDE).then_some(Self { dir })
    }

    /// What `HEAD` is, or `None` for a repository with no commit yet.
    pub fn head(&self) -> Option<Head> {
        Head::read(&Git::at(&self.dir))
    }

    /// What the log says about each file under the site root, and about who
    /// changed it when `contributors` asks.
    pub fn history(&self, contributors: bool) -> History {
        History::read(&Git::at(&self.dir), contributors)
    }
}
