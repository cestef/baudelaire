//! Scaffolding a project or a page: the files `init` and `new` write.

pub(super) mod draft;
pub(super) mod init;
pub(super) mod templates;
pub(super) mod vcs;

use std::path::{Path, PathBuf};

use owo_colors::OwoColorize;

use crate::config::Config;
use crate::error::Result;
use crate::error::warning::ScaffoldExists;
use crate::fs;
use crate::ui::{Paths, Ui};

/// Declarative scaffold: the files to create under a root, written in one pass
/// so nothing is laid down until every one of them is known.
pub(super) struct Scaffold<'a> {
    root: &'a Path,
    files: Vec<(PathBuf, String)>,
}

impl<'a> Scaffold<'a> {
    /// The ignore file every scaffold writes, and its contents. Unconditional,
    /// because the two directories it names (`public/`, `.baudelaire/`) are
    /// build output either way: it used to ride along with `--vcs`, so a
    /// scaffold that ran `git init` itself, afterwards, committed both.
    const IGNORE: &'static str = ".gitignore";
    const IGNORED: &'static str = include_str!("../scaffold/gitignore");

    fn new(root: &'a Path) -> Self {
        Self {
            root,
            files: Vec::new(),
        }
    }

    /// Add the ignore file both supported version-control systems read.
    fn ignore(self) -> Self {
        self.file(Self::IGNORE, Self::IGNORED)
    }

    fn file(mut self, rel: impl Into<PathBuf>, contents: impl Into<String>) -> Self {
        self.files.push((rel.into(), contents.into()));
        self
    }

    fn apply(self, ui: &Ui) -> Result<()> {
        for (rel, contents) in &self.files {
            let full = self.root.join(rel);
            // never clobber: `init` into an existing project must not overwrite
            // its config or templates. existing files are skipped with a warning.
            if full.exists() {
                ui.warn(ScaffoldExists { path: rel.clone() });
                continue;
            }
            if let Some(parent) = full.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(&full, contents)?;
            ui.detail(format_args!(
                "{} {}",
                "+".green(),
                Paths(&rel.display().to_string())
            ));
        }
        Ok(())
    }
}

impl Config {
    /// The collection a content path falls into by convention: the top-level
    /// directory under the content root. `None` for a file directly under it (a
    /// root page, which belongs to no collection). Mirrors discovery's
    /// convention so `new` infers the same collection the build later will.
    fn collection_for(&self, path: &Path) -> Option<String> {
        let rel = path.strip_prefix(&self.paths.content).unwrap_or(path);
        let mut components = rel.components();
        match (components.next(), components.next()) {
            (Some(dir), Some(_)) => Some(dir.as_os_str().to_str()?.to_owned()),
            _ => None,
        }
    }

    /// The basename `new` treats as a page bundle's index, both when writing one
    /// (`--bundle`) and when reading a title back off one. The configured
    /// [`crate::config::ContentConfig::index`], or the same `index` the build
    /// falls back to, in one place rather than at each of the two call sites.
    pub(super) fn bundle_index(&self) -> &str {
        self.index()
    }

    /// The template a scaffolded page names: whatever the build would resolve
    /// for it ([`Config::template_for`]), so `new` writes the binding the build
    /// will later pick rather than a second opinion about it. `None` when the
    /// config binds none, in which case the page is written without the key
    /// rather than against a filename this module made up.
    ///
    /// A root page resolves under [`ROOT`], the collection discovery puts it in,
    /// so `_root { template }` reaches a scaffolded page as it reaches a built
    /// one.
    fn scaffold_template(&self, collection: Option<&str>) -> Option<String> {
        self.template_for(collection.unwrap_or(crate::content::ROOT), None)
    }
}
