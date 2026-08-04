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

use std::path::{Path, PathBuf};

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
    /// The Typst import root of its templates: a package spec, or a
    /// root-absolute project path.
    import: String,
}

impl Theme {
    /// The directory names a theme uses, fixed because a theme cannot know what
    /// the project renamed its own to.
    const TEMPLATES: &'static str = "templates";
    const ASSETS: &'static str = "assets";
    const STATIC: &'static str = "static";
    const CONFIG: &'static str = "theme.kdl";

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
            // A package spec is itself a path root to the compiler, so this
            // resolves wherever the store happened to unpack it.
            import: parsed.to_string(),
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
            import: format!("/{}", rel.path().display()),
        })
    }

    /// Where the theme's files are.
    pub fn root(&self) -> &Path {
        &self.root
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
        format!("{}/{}", self.import, Self::TEMPLATES)
    }
}

/// A theme shipped inside the binary, ready to be written into a project.
///
/// A theme is files, and the four the project ships are small enough to carry
/// (about 60 KiB each) that "where do I get one" stops being a step. The
/// alternative was a download, which would make adopting a look the one thing
/// in the tool that needs the network, and a second package manager beside the
/// Typst one.
///
/// Whether a copied theme stays in step with the binary is the project's to
/// decide, as it is for the scaffolded templates: the copy is yours the moment
/// it lands.
#[cfg(feature = "themes")]
pub struct Bundled {
    /// What `baudelaire theme add` accepts, and the directory the copy lands in.
    pub name: &'static str,
    /// The kind of site it is for, one line, as `theme list` prints it.
    pub about: &'static str,
    files: include_dir::Dir<'static>,
}

/// The themes this binary carries.
///
/// Single source of truth: the names `theme add` resolves, the rows `theme
/// list` prints, and the suggestion a missing `--theme` directory closes on all
/// read this one table, and each row's files are the directory itself, so a
/// theme cannot ship half-listed.
#[cfg(feature = "themes")]
pub const BUNDLED: &[Bundled] = &[
    Bundled {
        name: "albatros",
        about: "a blog: centred column, tags, reading time, light and dark",
        files: include_dir::include_dir!("$CARGO_MANIFEST_DIR/themes/albatros"),
    },
    Bundled {
        name: "spleen",
        about: "a blog, in a terminal: monospace, dark first, no JavaScript",
        files: include_dir::include_dir!("$CARGO_MANIFEST_DIR/themes/spleen"),
    },
    Bundled {
        name: "phares",
        about: "documentation: sidebar from your tree, search palette, on-page outline",
        files: include_dir::include_dir!("$CARGO_MANIFEST_DIR/themes/phares"),
    },
    Bundled {
        name: "paysage",
        about: "a portfolio: landing page, work grid, one case study per project",
        files: include_dir::include_dir!("$CARGO_MANIFEST_DIR/themes/paysage"),
    },
];

#[cfg(feature = "themes")]
impl Bundled {
    /// The theme `name` selects, or an error naming the ones there are.
    pub fn find(name: &str) -> Result<&'static Self> {
        BUNDLED.iter().find(|t| t.name == name).ok_or_else(|| {
            let names: Vec<&str> = BUNDLED.iter().map(|t| t.name).collect();
            ThemeError::unknown(
                name,
                crate::config::dispatch::Keys::of(&names).help(name, "themes"),
            )
            .into()
        })
    }

    /// Whether a `--theme` spec names one of these, so a diagnostic can say
    /// which command would produce it. The spec is a directory path, and its
    /// last segment is the name a copy would be known by.
    pub fn named_by(spec: &str) -> Option<&'static Self> {
        let name = spec.rsplit('/').next()?;
        BUNDLED.iter().find(|t| t.name == name)
    }

    /// Write the theme's files under `dir`, skipping any that are already
    /// there, and report what was written. An existing file is the author's:
    /// a second `theme add` over an edited copy must not silently undo it.
    pub fn install(&self, dir: &Path) -> Result<Vec<PathBuf>> {
        let mut written = Vec::new();
        for file in Self::walk(&self.files) {
            let dst = dir.join(file.path());
            if dst.exists() {
                continue;
            }
            if let Some(parent) = dst.parent() {
                crate::fs::create_dir_all(parent)?;
            }
            crate::fs::write(&dst, file.contents())?;
            written.push(file.path().to_path_buf());
        }
        written.sort();
        Ok(written)
    }

    /// Every file in the theme, however deep. `include_dir` walks one level at
    /// a time, and a theme nests (`templates/`, `assets/`, `highlight/`).
    fn walk(dir: &include_dir::Dir<'static>) -> Vec<&'static include_dir::File<'static>> {
        let mut out: Vec<&include_dir::File<'static>> = dir.files().collect();
        for sub in dir.dirs() {
            out.extend(Self::walk(sub));
        }
        out
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

#[cfg(all(test, feature = "themes"))]
mod bundled_tests {
    use super::{BUNDLED, Bundled};

    /// Every shipped theme is complete enough to be a theme: its defaults and
    /// at least one layout, so `theme add` cannot write a directory that fails
    /// on the first build.
    #[test]
    fn every_shipped_theme_carries_its_config_and_templates() {
        let tmp = tempfile::tempdir().expect("tempdir");
        for theme in BUNDLED {
            let dir = tmp.path().join(theme.name);
            let written = theme.install(&dir).expect("install");
            assert!(!written.is_empty(), "{} wrote nothing", theme.name);
            assert!(
                dir.join("theme.kdl").is_file(),
                "{}: no theme.kdl",
                theme.name
            );
            assert!(
                dir.join("templates")
                    .read_dir()
                    .is_ok_and(|d| d.count() > 0),
                "{}: no templates",
                theme.name
            );
            // A second run is a no-op rather than a silent overwrite of edits.
            assert!(theme.install(&dir).expect("reinstall").is_empty());
        }
    }

    /// The spec a config carries is a path; its last segment is the name.
    #[test]
    fn a_directory_spec_names_the_theme_it_would_hold() {
        assert_eq!(
            Bundled::named_by("themes/albatros").map(|t| t.name),
            Some("albatros")
        );
        assert_eq!(
            Bundled::named_by("albatros").map(|t| t.name),
            Some("albatros")
        );
        assert!(Bundled::named_by("themes/mine").is_none());
    }

    /// A typo names the real ones rather than failing bare.
    #[test]
    fn an_unknown_theme_suggests_a_shipped_one() {
        let Err(err) = Bundled::find("albatross") else {
            panic!("not a shipped theme");
        };
        let rendered = format!("{err:?}");
        assert!(rendered.contains("albatros"), "{rendered}");
    }
}
