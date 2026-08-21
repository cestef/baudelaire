//! `paths { }`: where each kind of source lives, and where the build lands.

use std::path::{Path, PathBuf};

use dispatch_derive::Table as Derive;

use crate::codegen::TypstFmt;
use crate::config::Value;
use crate::config::dispatch::Kind::Table;
use crate::config::dispatch::{Block, Section};
use crate::config::node::NodeExt;
use crate::config::vocab::rule;
use crate::error::ConfigError;

/// Directory layout, every entry relative to [`Config::root`](crate::config::Config::root).
#[derive(Debug, Clone, Hash, Derive)]
pub struct Paths {
    /// The content tree of `.typ` pages.
    #[key(path)]
    pub content: PathBuf,

    /// Where the built site is written.
    #[key(path)]
    pub dist: PathBuf,

    /// Assets that go through the pipeline: CSS, JS, images.
    ///
    /// Minified, bundled, fingerprinted.
    #[key(path)]
    pub assets: PathBuf,

    /// Files copied to the output verbatim, untouched by the pipeline.
    ///
    /// No processing, no fingerprint, no URL prefix.
    #[key(name = "static", path)]
    pub r#static: PathBuf,

    /// Where layouts and partials are imported from.
    #[key(path)]
    pub templates: PathBuf,

    /// Files a page may take as its body, each under a name: a page names the name, never the path.
    ///
    /// `sources { changelog "../CHANGELOG.md" }`. A page selects a source by
    /// name, never by path, so content can only reach files the config already
    /// offered it. Unlike every other entry here, a value may climb out of the
    /// project.
    #[key(custom(
        Table,
        |c: &Self| Value::each(&c.sources, |path| path.clone().into()),
        |c: &mut Self, n: &kdl::KdlNode, t: &str| {
            let mut seen: Vec<String> = Vec::new();
            for entry in n.block(t)?.nodes() {
                let name = entry.name().value();
                let span = NodeExt::span(entry);
                if !TypstFmt::bindable(name) {
                    return Err(ConfigError::not_an_identifier(t, name, span).into());
                }
                if seen.iter().any(|declared| declared == name) {
                    return Err(ConfigError::duplicate_id(t, "source", name, span).into());
                }
                seen.push(name.to_owned());
            }
            c.sources = n
                .pairs(t)?
                .into_iter()
                .map(|(name, path)| (name, PathBuf::from(path)))
                .collect();
            Ok(())
        },
    ))]
    pub sources: Vec<(String, PathBuf)>,
}

impl Paths {
    /// Every configured directory the build *reads*, paired with the key that
    /// names it, so a new `paths` entry is covered by adding it here alone.
    ///
    /// Read by [`swallowed`], the prune sweep, [`Filter::roots`] and
    /// [`Engine::outside`]. The order is the order a reader meets them, which is
    /// what the dev server's startup banner lists.
    ///
    /// [`swallowed`]: Paths::swallowed
    /// [`Filter::roots`]: crate::cli::serve::watch::Filter
    /// [`Engine::outside`]: crate::engine::Engine
    pub fn trees(&self) -> [(&'static str, &Path); 4] {
        [
            ("content", &self.content),
            ("templates", &self.templates),
            ("assets", &self.assets),
            ("static", &self.r#static),
        ]
    }

    /// The first source directory `dist` would contain, if any.
    ///
    /// The prune sweep deletes everything under `dist` the build did not write,
    /// so a `dist` holding the sources deletes the sources; by the time the
    /// sweep runs every path looks alike, so the config is where it is caught.
    ///
    /// Entries resolve against `root` rather than the process cwd.
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
    /// Both sides go through [`crate::fs::resolved`], because either can be
    /// reached through a symlink and a lexical comparison would leave a
    /// configured directory looking like it sits outside its own root.
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
/// [`Paths::under`]. Only the two typst reads.
///
/// A directory outside the root keeps its absolute path, there being no
/// root-relative spelling of it.
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

impl Paths {
    /// The file declared under `name`, if the site declared one.
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
