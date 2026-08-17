//! Editor support for the modules a build serves from memory: `baudelaire
//! packages` mirrors the `@baudelaire/*` and `baudelaire:*` families to disk so
//! an editor resolves them.
//!
//! Nothing here is read back by a build.

mod packages;
#[cfg(feature = "js")]
mod types;

use std::fmt::Display;
use std::path::{Path, PathBuf};

use miette::Diagnostic;
use owo_colors::OwoColorize;

use crate::config::Config;
use crate::error::Result;
use crate::generated::Generated;
use crate::ui::{Count, Level, Paths, Ui};

/// One thing a reader has to *know* once a family is mirrored; what a reader
/// has to *do* is a [`Setup`] instead.
type Advice = Box<dyn Diagnostic + Send + Sync>;

/// One family of generated modules, mirrored where an editor resolves it.
trait Target {
    /// What one of the family's modules is called in the command's own output,
    /// singular: [`Count`] does the plural.
    fn label(&self) -> &'static str;

    /// Everything this target would write for `mirror`, computed but not
    /// written.
    fn mirrored(&self, mirror: &Mirror) -> Result<Mirrored>;

    /// What an install of this target owns, and exactly what an uninstall
    /// removes; cheap, so an uninstall never generates a module's source.
    fn owned(&self, mirror: &Mirror) -> Result<PathBuf>;
}

fn builtin() -> Vec<Box<dyn Target>> {
    vec![
        Box::new(packages::Typst),
        #[cfg(feature = "js")]
        Box::new(types::Types),
    ]
}

/// What one target would write: the files, the modules to name in a report,
/// and anything the reader has to do next.
struct Mirrored {
    /// The base the files are written under: a package directory, or the
    /// project root for a file whose path is relative to it.
    base: PathBuf,
    generated: Box<dyn Generated>,
    /// One row per module, as a reader names it: `@baudelaire/pages`.
    modules: Vec<String>,
    /// The settings this family still needs to resolve in an editor.
    setup: Vec<Setup>,
    notes: Vec<Advice>,
}

/// One setting a reader has to make for a mirrored family to resolve, and the
/// tool that reads it.
struct Setup {
    tool: &'static str,
    value: String,
    /// Where else the same setting goes, dimmed under it. `None` when there is
    /// only one place for it.
    hint: Option<&'static str>,
}

/// A path as the reader would type it: relative to the project when it is
/// inside it, absolute when it is not (`--global`, or a `--path` elsewhere).
struct Shown<'a> {
    root: &'a Path,
    path: &'a Path,
}

impl Display for Shown<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let path = self.path.strip_prefix(self.root).unwrap_or(self.path);
        write!(f, "{}", Paths(&path.display().to_string()))
    }
}

struct Written {
    label: &'static str,
    path: PathBuf,
    modules: Vec<String>,
    notes: Vec<Advice>,
}

/// What a whole run wrote, and the shape it prints in.
pub struct Install {
    families: Vec<Written>,
    /// The settings the run still needs, gathered across the families so they
    /// print as one block rather than one per family.
    setup: Vec<Setup>,
    /// The project the paths are reported relative to.
    root: PathBuf,
}

impl Install {
    /// One result line per family: how many modules landed, and where, with the
    /// module names only under `-v`.
    pub fn render(self, ui: &Ui) -> Settings {
        let counts: Vec<String> = self
            .families
            .iter()
            .map(|family| Count::of(family.modules.len(), family.label).to_string())
            .collect();
        let width = counts.iter().map(String::len).max().unwrap_or_default();
        for (family, plain) in self.families.iter().zip(&counts) {
            ui.done(format_args!(
                "{}{}  {}",
                Count::of(family.modules.len(), family.label).styled(),
                " ".repeat(width - plain.len()),
                Shown {
                    root: &self.root,
                    path: &family.path
                }
            ));
            if ui.level() >= Level::Verbose {
                ui.tree(&family.modules);
            }
        }
        for note in self.families.into_iter().flat_map(|family| family.notes) {
            ui.report(note);
        }
        Settings(self.setup)
    }
}

/// The settings an install still needs, held back so a caller prints them where
/// they belong.
pub struct Settings(Vec<Setup>);

impl Settings {
    /// Print them as one aligned block under a heading, and nothing at all when
    /// there is nothing to set.
    pub fn render(&self, ui: &Ui) {
        if self.0.is_empty() {
            return;
        }
        ui.section("editor setup");
        for setting in &self.0 {
            ui.arrow(setting.tool, &setting.value);
            if let Some(hint) = setting.hint {
                ui.item(hint.dimmed());
            }
        }
    }
}

/// What a whole run removed; nothing to remove is not an error.
pub struct Removal {
    families: Vec<Removed>,
    root: PathBuf,
}

impl Removal {
    pub fn render(&self, ui: &Ui) {
        for family in &self.families {
            match &family.path {
                Some(path) => ui.done(format_args!(
                    "removed {} from {}",
                    family.plural(),
                    Shown {
                        root: &self.root,
                        path
                    }
                )),
                None => ui.detail(format_args!("no {} to remove", family.plural())),
            }
        }
    }
}

struct Removed {
    label: &'static str,
    path: Option<PathBuf>,
}

impl Removed {
    /// The family named as a group, since an uninstall takes all of them or
    /// none.
    fn plural(&self) -> String {
        format!("{}s", self.label)
    }
}

/// One mirroring run: the project it mirrors, and where its targets write.
pub struct Mirror<'a> {
    config: &'a Config,
    /// `--path`: where the packages go instead of the default.
    dir: Option<&'a Path>,
    /// `--global`: put the packages in typst's own package directory, shared by
    /// every project on the machine.
    global: bool,
}

impl<'a> Mirror<'a> {
    pub fn new(config: &'a Config, dir: Option<&'a Path>, global: bool) -> Self {
        Self {
            config,
            dir,
            global,
        }
    }

    pub fn install(&self) -> Result<Install> {
        let mut families = Vec::new();
        let mut setup = Vec::new();
        for target in &builtin() {
            let mirrored = target.mirrored(self)?;
            mirrored.generated.write(&mirrored.base)?;
            setup.extend(mirrored.setup);
            families.push(Written {
                label: target.label(),
                path: target.owned(self)?,
                modules: mirrored.modules,
                notes: mirrored.notes,
            });
        }
        Ok(Install {
            families,
            setup,
            root: self.config.root.clone(),
        })
    }

    /// Remove what an install wrote, and nothing beside it.
    pub fn uninstall(&self) -> Result<Removal> {
        let families = builtin()
            .iter()
            .map(|target| {
                let owned = target.owned(self)?;
                Ok(Removed {
                    label: target.label(),
                    path: Self::discard(&owned)?.then_some(owned),
                })
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(Removal {
            families,
            root: self.config.root.clone(),
        })
    }

    /// Remove one target's own path, whether that is a directory of packages or
    /// a single declaration file. `false` when there was nothing there.
    fn discard(path: &Path) -> Result<bool> {
        if !path.exists() {
            return Ok(false);
        }
        if path.is_dir() {
            crate::fs::remove_dir_all(path)?;
        } else {
            crate::fs::remove_file(path)?;
        }
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::module::Packages;

    fn mirror<'a>(project: &'a Config, dir: &'a Path) -> Mirror<'a> {
        Mirror::new(project, Some(dir), false)
    }

    fn project(root: &Path) -> Config {
        Config {
            root: root.to_path_buf(),
            ..Config::default()
        }
    }

    #[test]
    fn a_mirrored_module_resolves_the_way_typst_resolves_a_package() {
        use ::typst::syntax::package::PackageSpec;
        use typst_kit::packages::FsPackages;

        let dir = tempfile::tempdir().unwrap();
        let root = tempfile::tempdir().unwrap();
        let config = project(root.path());
        mirror(&config, dir.path()).install().expect("install");

        let packages = FsPackages::new(dir.path());
        for package in Packages::new(&config).packages() {
            let spec = PackageSpec {
                namespace: "baudelaire".into(),
                name: package.name.into(),
                version: "0.1.0".parse().expect("a version typst parses"),
            };
            assert!(
                packages.obtain(&spec).is_some(),
                "typst cannot resolve {}",
                package.specifier()
            );
        }
    }

    #[test]
    fn every_family_reports_where_it_wrote_and_what_is_in_it() {
        let dir = tempfile::tempdir().unwrap();
        let root = tempfile::tempdir().unwrap();
        let config = project(root.path());

        let written = mirror(&config, dir.path()).install().expect("install");

        assert_eq!(written.families.len(), builtin().len());
        for family in &written.families {
            assert!(family.path.exists(), "{} wrote nothing", family.label);
            assert!(!family.modules.is_empty(), "{} named nothing", family.label);
        }
    }

    #[test]
    fn uninstalling_leaves_everything_it_does_not_own_alone() {
        let dir = tempfile::tempdir().unwrap();
        let root = tempfile::tempdir().unwrap();
        let config = project(root.path());
        let neighbour = dir.path().join("local").join("theme").join("0.1.0");
        std::fs::create_dir_all(&neighbour).unwrap();
        mirror(&config, dir.path()).install().expect("install");

        let removed = mirror(&config, dir.path()).uninstall().expect("uninstall");

        assert!(removed.families.iter().all(|f| f.path.is_some()));
        assert!(!Packages::namespace(dir.path()).exists());
        assert!(neighbour.exists());
        let again = mirror(&config, dir.path()).uninstall().expect("uninstall");
        assert!(again.families.iter().all(|f| f.path.is_none()));
    }

    #[test]
    fn two_projects_do_not_share_a_mirror() {
        let one = tempfile::tempdir().unwrap();
        let two = tempfile::tempdir().unwrap();
        let (first, second) = (project(one.path()), project(two.path()));

        for config in [&first, &second] {
            Mirror::new(config, None, false).install().expect("install");
        }

        for config in [&first, &second] {
            let served = config
                .root
                .join(Packages::project())
                .join("baudelaire/site/0.1.0/lib.typ");
            assert!(served.exists(), "{} has no mirror", config.root.display());
        }
    }

    #[test]
    fn the_tables_mirror_empty_before_a_first_build() {
        let dir = tempfile::tempdir().unwrap();
        let root = tempfile::tempdir().unwrap();
        let config = project(root.path());

        let written = mirror(&config, dir.path()).install().expect("install");

        let typst = written.families.first().expect("the typst family is first");
        assert!(
            typst.notes.iter().any(|note| {
                note.code()
                    .is_some_and(|code| code.to_string() == "baudelaire::mirror::unbuilt")
            }),
            "an unbuilt project was not told its tables are empty"
        );
        let source =
            std::fs::read_to_string(Packages::namespace(dir.path()).join("sections/0.1.0/lib.typ"))
                .unwrap();
        assert!(source.contains("#let sections(lang)"));
    }
}
