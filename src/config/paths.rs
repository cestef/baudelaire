//! `paths { }`: where each kind of source lives, and where the build lands.

use std::path::{Path, PathBuf};

use crate::config::dispatch::Kind::Path as Directory;
use crate::config::dispatch::Kind::Table;
use crate::config::dispatch::{Block, Section};
use crate::config::node::NodeExt;

/// Directory layout, every entry relative to [`Config::root`](crate::config::Config::root).
#[derive(Debug, Clone, Hash)]
pub struct Paths {
    /// Content source directory.
    pub content: PathBuf,
    /// Output (distribution) directory.
    pub dist: PathBuf,
    /// Asset pipeline source directory (minified, bundled, fingerprinted).
    pub assets: PathBuf,
    /// Static passthrough directory: copied verbatim to the `dist` root, with no
    /// processing, no fingerprint, no URL prefix.
    pub r#static: PathBuf,
    /// Layout / template directory.
    pub templates: PathBuf,
    /// Files a page may adopt as its body, each under a name of the site's
    /// choosing: `sources { changelog "../CHANGELOG.md" }`.
    ///
    /// Declared here and named nowhere else, which is the whole security model
    /// of the feature. A page selects a source *by name*, so content -- the part
    /// of a site a pull request can touch -- can never name a path, and cannot
    /// reach a file the config did not already offer it. `paths` is also one of
    /// the sections a theme is refused ([`Config::OWNED`]), so a fetched theme
    /// cannot introduce one either.
    ///
    /// Unlike every other entry here, a value may climb out of the project: a
    /// repository whose site lives in `docs/` publishes its `../CHANGELOG.md`,
    /// and refusing that would leave the copy-it-in workaround the feature
    /// exists to remove. It is the config's to allow, on the same footing as
    /// `hooks`, which can already run any command at all.
    ///
    /// [`Config::OWNED`]: crate::config::Config
    pub sources: Vec<(String, PathBuf)>,
}

impl Paths {
    /// Every configured directory the build *reads*, paired with the key that
    /// names it. The single list of what [`dist`](Paths::dist) must stay clear
    /// of, walked by both the containment guard ([`swallowed`]) and the prune
    /// sweep, so a new `paths` entry is covered by adding it here alone.
    ///
    /// [`swallowed`]: Paths::swallowed
    pub fn trees(&self) -> [(&'static str, &Path); 4] {
        [
            ("content", &self.content),
            ("assets", &self.assets),
            ("static", &self.r#static),
            ("templates", &self.templates),
        ]
    }

    /// The first source directory `dist` would contain, if any.
    ///
    /// The prune sweep deletes everything under `dist` the build did not write,
    /// so a `dist` holding the sources deletes the sources: `paths { dist "." }`
    /// took `config.kdl` and the whole content tree with it, and reported a
    /// successful build. Refusing the config is the only place this can be
    /// caught, since by the time the sweep runs every path looks alike.
    ///
    /// Entries resolve against `root` rather than the process cwd, so a caller
    /// that has not changed into the project still gets the right answer.
    pub fn swallowed(&self, root: &Path) -> Option<(&'static str, &Path)> {
        let dist = crate::fs::resolved(root.join(&self.dist));
        self.trees()
            .into_iter()
            .find(|(_, path)| crate::fs::resolved(root.join(path)).starts_with(&dist))
    }

    /// The directories typst sees, as *it* spells them: relative to the project
    /// root, which is how a span, a dependency path and an import all name a
    /// file.
    ///
    /// Both sides go through [`crate::fs::resolved`], the spelling the link map
    /// and the dependency tracker already key on, because either can be reached
    /// through a symlink: comparing them lexically leaves a configured directory
    /// looking like it sits outside the very root it is under.
    pub fn under(&self, root: &Path) -> Rooted {
        let root = crate::fs::resolved(root);
        let relative = |dir: &Path| {
            let dir = crate::fs::resolved(dir);
            dir.strip_prefix(&root)
                .map_or_else(|_| dir.clone(), Path::to_path_buf)
        };
        Rooted {
            content: relative(&self.content),
            templates: relative(&self.templates),
        }
    }
}

/// The configured source directories in the compiler's spelling, from
/// [`Paths::under`]. Only the two typst reads: `dist`, `assets` and `static` are
/// walked by the build itself and never named in a span or an import.
///
/// A directory outside the root keeps its absolute path: there is no
/// root-relative spelling of it, and inventing one would name a different place.
pub struct Rooted {
    /// Where pages are authored: what a link's origin is tested against to tell
    /// an author's own reference from a layout's chrome.
    pub content: PathBuf,
    /// Where layouts live: what a wrapper's root-absolute `#import` resolves
    /// against.
    pub templates: PathBuf,
}

impl Default for Paths {
    fn default() -> Self {
        Self {
            content: PathBuf::from("content"),
            dist: PathBuf::from("public"),
            assets: PathBuf::from("assets"),
            r#static: PathBuf::from("static"),
            templates: PathBuf::from("templates"),
            sources: Vec::new(),
        }
    }
}

/// The `paths { .. }` section: directory layout knobs.
impl Section for Paths {
    const RULES: Block<Self> = Block(&[
        (
            "content",
            Directory,
            "The content tree of `.typ` pages.",
            |c, n, t| {
                c.content = n.string(t, 0)?.into();
                Ok(())
            },
        ),
        (
            "dist",
            Directory,
            "Where the built site is written.",
            |c, n, t| {
                c.dist = n.string(t, 0)?.into();
                Ok(())
            },
        ),
        (
            "assets",
            Directory,
            "Assets that go through the pipeline: CSS, JS, images.",
            |c, n, t| {
                c.assets = n.string(t, 0)?.into();
                Ok(())
            },
        ),
        (
            "static",
            Directory,
            "Files copied to the output verbatim, untouched by the pipeline.",
            |c, n, t| {
                c.r#static = n.string(t, 0)?.into();
                Ok(())
            },
        ),
        (
            "templates",
            Directory,
            "Where layouts and partials are imported from.",
            |c, n, t| {
                c.templates = n.string(t, 0)?.into();
                Ok(())
            },
        ),
        (
            "sources",
            Table,
            "Files a page may take as its body, each under a name: a page names the name, never the path.",
            |c, n, t| {
                c.sources = n
                    .pairs(t)?
                    .into_iter()
                    .map(|(name, path)| (name, PathBuf::from(path)))
                    .collect();
                Ok(())
            },
        ),
    ]);
}

impl Paths {
    /// The file declared under `name`, if the site declared one.
    ///
    /// A linear scan: a site declares a handful of these, and the order they
    /// were written in is worth keeping for the diagnostic that lists them.
    pub fn source(&self, name: &str) -> Option<&Path> {
        self.sources
            .iter()
            .find(|(declared, _)| declared == name)
            .map(|(_, path)| path.as_path())
    }

    /// The names declared, for the error that reports one that is not.
    pub fn declared(&self) -> Vec<&str> {
        self.sources.iter().map(|(name, _)| name.as_str()).collect()
    }
}
