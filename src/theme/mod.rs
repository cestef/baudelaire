//! Themes: a site's templates, assets, and defaults, shipped as one unit.
//!
//! A published theme is named the way any Typst dependency is
//! (`@preview/plume:1.0.0`) and resolved through the same package store the
//! compiler already uses, so it is downloaded once, cached across projects, and
//! versioned by the registry. No second package manager, no vendoring, no
//! submodule. A theme being *written* is named by its directory instead, so the
//! author can edit it in place and rebuild.
//!
//! Inside, a theme is laid out like a site, with fixed directory names: the
//! project's `paths` configure the *project*, and a theme cannot know what they
//! were changed to.
//!
//! ```text
//! templates/   layouts a page can be bound to
//! assets/      stylesheets, scripts, images
//! static/      files copied verbatim
//! theme.kdl    config defaults the site's own config overrides
//! ```
//!
//! Everything a theme provides is a *default*: the project's file at the same
//! relative path wins, and its config wins key by key.
//!
//! This module resolves the theme a *build* uses. Getting one into a project in
//! the first place is [`source`] (where the files come from) and [`install`]
//! (writing them in, and telling your bytes from ours) — both behind the
//! `themes` feature, with the command that drives them.

use std::path::{Path, PathBuf};

#[cfg(feature = "themes")]
mod bundled;
#[cfg(feature = "themes")]
mod install;
#[cfg(feature = "themes")]
mod source;

#[cfg(feature = "themes")]
pub use bundled::{BUNDLED, Bundled};
#[cfg(feature = "themes")]
pub use install::{Lock, State, Tracked, present, uninstall};
#[cfg(feature = "themes")]
pub use source::{Fetched, Origin, Source};

use typst::syntax::package::PackageSpec;
use typst_kit::packages::SystemPackages;

use crate::config::Config;
use crate::error::{Result, ThemeError};
use crate::fs::Contained;
use crate::world::Registry;

/// A resolved theme: where its files are, and how Typst names them.
#[derive(Debug, Clone)]
pub struct Theme {
    /// Where the theme's files live, for the layered asset and static trees.
    root: PathBuf,
    /// How a layout of this theme is imported.
    import: Import,
}

/// How Typst reaches a theme's files.
///
/// The two cases are not two spellings of one path. A directory theme is inside
/// the project, so its layouts import by project path; a package's files are not
/// reachable by *any* import string, so they are served under a project path
/// instead: see [`Theme::mount`].
#[derive(Debug, Clone)]
enum Import {
    /// A directory inside the project, as a root-absolute project path.
    Project(String),
    /// A package, served under the mount point.
    Mounted,
}

impl Theme {
    /// The directory names a theme uses, fixed because a theme cannot know what
    /// the project renamed its own to.
    pub const TEMPLATES: &'static str = "templates";
    const ASSETS: &'static str = "assets";
    const STATIC: &'static str = "static";
    pub const CONFIG: &'static str = "theme.kdl";

    /// The scratch subdirectory a *package* theme's own root is served under,
    /// so that its layouts can be imported at all. See [`Theme::mount`].
    const MOUNT: &'static str = "theme";

    /// The theme a config names, if it names one: every field the resolution
    /// reads lives on the config, so the two callers (the config's own theme
    /// layering, and the engine) cannot disagree about how a spec is read.
    pub fn of(config: &Config) -> Result<Option<Self>> {
        config
            .theme
            .as_deref()
            .map(|spec| Self::resolve(spec, &config.root, config.typst.registry.as_deref()))
            .transpose()
    }

    /// Resolve the configured `theme` value.
    ///
    /// A leading `@` means a package, resolved (and downloaded if needed)
    /// through the package store. Anything else is a directory inside the
    /// project, which is how a theme is developed before it is published.
    fn resolve(theme: &str, project: &Path, registry: Option<&str>) -> Result<Self> {
        match theme.starts_with('@') {
            true => Self::package(theme, registry),
            false => Self::directory(theme, project),
        }
    }

    /// A published theme, from the package store.
    fn package(spec: &str, registry: Option<&str>) -> Result<Self> {
        let parsed: PackageSpec = spec
            .parse()
            .map_err(|why: typst::ecow::EcoString| ThemeError::spec(spec, why))?;
        let packages = SystemPackages::from(Registry(registry));
        let root = packages
            .obtain(&parsed)
            .map_err(|why| ThemeError::unavailable(spec, why))?;
        Ok(Self {
            root: root.path().to_path_buf(),
            import: Import::Mounted,
        })
    }

    /// A theme being developed, from a directory inside the project.
    ///
    /// Inside, because a Typst import cannot reach outside the project root: a
    /// theme elsewhere on the disk would resolve its assets but fail on its very
    /// first template, which is a worse way to find out.
    fn directory(path: &str, project: &Path) -> Result<Self> {
        let rel = Contained::new(path).ok_or_else(|| ThemeError::outside(path))?;
        let root = rel.under(project);
        if !root.is_dir() {
            return Err(ThemeError::missing(path).into());
        }
        Ok(Self {
            root,
            import: Import::Project(format!("/{}", rel.path().display())),
        })
    }

    /// Where the theme's files are.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The project path a package theme's root is served under, and the
    /// directory it is served from: `None` for a directory theme, whose files
    /// are already in the project.
    ///
    /// A Typst import string cannot name a file *inside* a package: everything
    /// after the `:` is read as a version, so
    /// `@local/plume:0.1.0/templates/page.typ` fails as `0/templates/page is
    /// not a valid patch version`, and a package theme's layouts would be
    /// unreachable. The package's root is mounted under the project instead, so
    /// a layout import is an ordinary path import and everything a theme does
    /// relative to itself (`../parts.typ`, a `show raw` palette) keeps working.
    ///
    /// Nothing is copied: the mount is served straight out of the package store
    /// by [`crate::world::Project`], so there is no vendored second copy to go
    /// stale and nothing to clean up.
    ///
    /// It lives under the scratch directory, the one path a project has already
    /// ceded to baudelaire, and shadows anything a site put there.
    pub fn mount(&self) -> Option<(String, &Path)> {
        match self.import {
            Import::Project(_) => None,
            Import::Mounted => Some((Self::mounted(), &self.root)),
        }
    }

    /// The mount point as Typst spells a path: project-rooted, `/`-joined, no
    /// leading slash, so it can be both matched against a file id's virtual
    /// path and written into an import.
    fn mounted() -> String {
        format!("{}/{}", Config::SCRATCH, Self::MOUNT)
    }

    /// The theme's asset directory, whether or not it exists.
    pub fn assets(&self) -> PathBuf {
        self.root.join(Self::ASSETS)
    }

    /// The theme's static-passthrough directory, whether or not it exists.
    pub fn statics(&self) -> PathBuf {
        self.root.join(Self::STATIC)
    }

    /// The theme's `theme.kdl`, if it ships one.
    pub fn config(&self) -> Option<PathBuf> {
        let path = self.root.join(Self::CONFIG);
        path.is_file().then_some(path)
    }

    /// Whether the theme carries `templates/<file>`, which decides whether a
    /// page's layout import points into the theme or into the project.
    pub fn has_template(&self, file: &str) -> bool {
        self.root.join(Self::TEMPLATES).join(file).is_file()
    }

    /// The Typst import root for the theme's templates: its own root plus the
    /// template directory, which is what a layout import is written against.
    pub fn templates(&self) -> String {
        let root = match &self.import {
            Import::Project(path) => path.clone(),
            Import::Mounted => format!("/{}", Self::mounted()),
        };
        format!("{root}/{}", Self::TEMPLATES)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project() -> tempfile::TempDir {
        let tmp = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir_all(tmp.path().join("themes/plume/templates")).expect("mkdir");
        tmp
    }

    /// A malformed package spec is rejected where it is written, not as a
    /// mysterious package-not-found later on.
    #[test]
    fn a_malformed_package_spec_is_an_error() {
        let tmp = project();
        for spec in ["@preview/plume", "@plume", "@preview/plume:x"] {
            assert!(
                Theme::resolve(spec, tmp.path(), None).is_err(),
                "{spec} should not parse"
            );
        }
    }

    /// A directory theme imports by project-relative path, which is what the
    /// compiler can actually resolve.
    #[test]
    fn a_directory_theme_imports_by_project_path() {
        let tmp = project();
        let theme = Theme::resolve("themes/plume", tmp.path(), None).expect("theme");
        assert_eq!(theme.templates(), "/themes/plume/templates");
        assert_eq!(theme.root(), tmp.path().join("themes/plume"));
    }

    /// A Typst import cannot leave the project root, so a theme that would sit
    /// outside it is refused up front rather than half-working. An empty name is
    /// refused with them: it used to resolve the theme to the project root
    /// itself, making every project file a theme file.
    #[test]
    fn a_theme_outside_the_project_is_refused() {
        let tmp = project();
        for path in ["../elsewhere", "/etc/theme", "themes/../../up", "", "."] {
            assert!(
                Theme::resolve(path, tmp.path(), None).is_err(),
                "{path} should be refused"
            );
        }
    }

    #[test]
    fn a_missing_directory_is_an_error() {
        let tmp = project();
        assert!(Theme::resolve("themes/absent", tmp.path(), None).is_err());
    }

    #[test]
    fn template_lookup_sees_only_files_the_theme_has() {
        let tmp = project();
        std::fs::write(tmp.path().join("themes/plume/templates/page.typ"), "").expect("write");
        let theme = Theme::resolve("themes/plume", tmp.path(), None).expect("theme");
        assert!(theme.has_template("page.typ"));
        assert!(!theme.has_template("post.typ"));
    }
}
