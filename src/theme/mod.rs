//! Themes: a site's templates, assets, and defaults, shipped as one unit.
//!
//! ```text
//! templates/   layouts a page can be bound to
//! assets/      stylesheets, scripts, images
//! static/      files copied verbatim
//! theme.kdl    config defaults the site's own config overrides
//! ```
//!
//! Everything a theme provides is a default: the project's file at the same
//! relative path wins, and its config wins key by key.

use std::path::{Path, PathBuf};

#[cfg(feature = "themes")]
mod archive;
#[cfg(feature = "themes")]
mod bundled;
#[cfg(feature = "themes")]
mod forge;
#[cfg(feature = "themes")]
mod install;
#[cfg(feature = "themes")]
mod local;
#[cfg(feature = "themes")]
mod package;
#[cfg(feature = "themes")]
mod source;

#[cfg(feature = "themes")]
pub use bundled::{BUNDLED, Bundled};
#[cfg(feature = "themes")]
pub use install::{Lock, State, Tracked};
#[cfg(feature = "themes")]
pub use source::{Fetched, Fetching, Origin, Source};

use typst::syntax::package::PackageSpec;
use typst_kit::packages::SystemPackages;

use crate::config::Config;
use crate::error::{Result, ThemeError};
use crate::fs::Contained;
use crate::world::Registry;

/// A resolved theme: where its files are, and how Typst names them.
#[derive(Debug, Clone)]
pub struct Theme {
    root: PathBuf,
    import: Import,
}

/// How Typst reaches a theme's files.
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

    /// The scratch subdirectory a package theme's root is served under, so its
    /// layouts can be imported at all.
    const MOUNT: &'static str = "theme";

    /// The theme a config names, if it names one.
    pub fn of(config: &Config) -> Result<Option<Self>> {
        config
            .theme
            .as_deref()
            .map(|spec| Self::resolve(spec, &config.root, config.typst.registry.as_deref()))
            .transpose()
    }

    /// Resolve the configured `theme` value: a leading `@` is a package,
    /// anything else a directory inside the project.
    fn resolve(theme: &str, project: &Path, registry: Option<&str>) -> Result<Self> {
        if theme.starts_with('@') {
            Self::package(theme, registry)
        } else {
            Self::directory(theme, project)
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
    /// Inside, because a Typst import cannot reach outside the project root.
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

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The project path a package theme's root is served under, and the
    /// directory it is served from: `None` for a directory theme, whose files
    /// are already in the project.
    ///
    /// A Typst import string cannot name a file inside a package, since
    /// everything after the `:` is read as a version, so the package's root is
    /// mounted under the project instead.
    pub fn mount(&self) -> Option<(String, &Path)> {
        match self.import {
            Import::Project(_) => None,
            Import::Mounted => Some((Self::mounted(), &self.root)),
        }
    }

    /// The mount point as Typst spells a path: project-rooted, `/`-joined, no
    /// leading slash.
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

    /// Whether the theme carries `templates/<file>`.
    pub fn has_template(&self, file: &str) -> bool {
        self.root.join(Self::TEMPLATES).join(file).is_file()
    }

    /// The Typst import root a layout import of this theme is written against.
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

    #[test]
    fn a_directory_theme_imports_by_project_path() {
        let tmp = project();
        let theme = Theme::resolve("themes/plume", tmp.path(), None).expect("theme");
        assert_eq!(theme.templates(), "/themes/plume/templates");
        assert_eq!(theme.root(), tmp.path().join("themes/plume"));
    }

    /// An empty name is refused with them: it resolves the theme to the project
    /// root itself, making every project file a theme file.
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
