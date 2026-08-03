//! Editor support for the modules a build serves from memory.
//!
//! A build answers `@baudelaire/*` (typst) and `baudelaire:*` (JavaScript) from
//! memory, so nothing on disk holds either family and an editor marks every
//! import unknown. `baudelaire packages` mirrors both to disk, and `init` does
//! it for a fresh project.
//!
//! One [`Target`] per family is the single source of truth for what a mirror
//! does: where the family is written, what it writes there, what it reports,
//! and what an uninstall may remove. The families keep their own knowledge (the
//! typst package layout is [`crate::world::module::package`]'s, the declaration
//! text is [`crate::engine::asset::Declarations`]'s); nothing about *mirroring*
//! lives outside this module, so the CLI runs one loop over
//! [`builtin`] rather than a branch per family.
//!
//! Nothing here is read by a build, which is what makes a stale mirror safe: it
//! can mislead an editor and can never change a page.

mod packages;
#[cfg(feature = "js")]
mod types;

use std::path::{Path, PathBuf};

use std::fmt::Display;

use miette::Diagnostic;

use crate::config::Config;
use crate::error::Result;
use crate::generated::Generated;
use crate::ui::Paths;

/// One thing a reader has to do or know once a family is mirrored, as a typed
/// advice diagnostic: rendered with the warnings, counted against nothing.
type Advice = Box<dyn Diagnostic + Send + Sync>;

/// One family of generated modules, mirrored where an editor resolves it.
///
/// Adding a family is one impl and one line in [`builtin`]. A family compiled
/// out is a row that is not there, which is why the `js` feature is spelled
/// here rather than at the call sites.
trait Target {
    /// What the family is called in the command's own output.
    fn label(&self) -> &'static str;

    /// Everything this target would write for `mirror`, computed but not
    /// written. One method rather than one per question, so a target cannot
    /// answer "where" and "what" from two different places.
    fn mirrored(&self, mirror: &Mirror) -> Result<Mirrored>;

    /// What an install of this target owns: what a run reports as the place it
    /// wrote, and exactly what an uninstall removes. Cheap on purpose, so an
    /// uninstall never generates a module's source.
    fn owned(&self, mirror: &Mirror) -> Result<PathBuf>;
}

/// The registered families.
fn builtin() -> Vec<Box<dyn Target>> {
    vec![
        Box::new(packages::Typst),
        #[cfg(feature = "js")]
        Box::new(types::Types),
    ]
}

/// What one target would write: the files, the modules to name in a report,
/// and anything the reader has to do next. Where they land is
/// [`Target::owned`], which an uninstall reads too.
struct Mirrored {
    /// The base the files are written under: a package directory, or the
    /// project root for a file whose path is relative to it.
    base: PathBuf,
    /// The family's own producer, so a mirror never rebuilds what a family
    /// already knows how to emit.
    generated: Box<dyn Generated>,
    /// One row per module, as a reader names it: `@baudelaire/pages`.
    modules: Vec<String>,
    /// What to point an editor at, what is empty until a first build.
    notes: Vec<Advice>,
}

/// What one target wrote, and the line a run reports it with.
pub struct Written {
    label: &'static str,
    path: PathBuf,
    /// One row per module, for the caller to list under the line.
    pub modules: Vec<String>,
    /// What the reader has to do or know next.
    pub notes: Vec<Advice>,
}

impl Display for Written {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "mirrored {} {} into {}",
            self.modules.len(),
            self.label,
            Paths(&self.path.display().to_string())
        )
    }
}

/// What one target removed, and the line a run reports it with. Nothing to
/// remove is not an error: a second uninstall is a no-op, and says so.
pub struct Removed {
    label: &'static str,
    path: Option<PathBuf>,
}

impl Display for Removed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.path {
            Some(path) => write!(
                f,
                "removed {} from {}",
                self.label,
                Paths(&path.display().to_string())
            ),
            None => write!(f, "no {} to remove", self.label),
        }
    }
}

/// One mirroring run: the project it mirrors, and where its targets write.
pub struct Mirror<'a> {
    /// The project whose data the modules are generated from, and whose `root`
    /// a project-local target writes under.
    config: &'a Config,
    /// `--path`: where a machine-global target writes instead of its default.
    dir: Option<&'a Path>,
}

impl<'a> Mirror<'a> {
    pub fn new(config: &'a Config, dir: Option<&'a Path>) -> Self {
        Self { config, dir }
    }

    /// Write every family, reporting what landed where.
    pub fn install(&self) -> Result<Vec<Written>> {
        builtin()
            .iter()
            .map(|target| {
                let mirrored = target.mirrored(self)?;
                mirrored.generated.write(&mirrored.base)?;
                Ok(Written {
                    label: target.label(),
                    path: target.owned(self)?,
                    modules: mirrored.modules,
                    notes: mirrored.notes,
                })
            })
            .collect()
    }

    /// Remove what an install wrote, and nothing beside it.
    pub fn uninstall(&self) -> Result<Vec<Removed>> {
        builtin()
            .iter()
            .map(|target| {
                let owned = target.owned(self)?;
                Ok(Removed {
                    label: target.label(),
                    path: Self::discard(&owned)?.then_some(owned),
                })
            })
            .collect()
    }

    /// Remove one target's own path, whether that is a directory of packages or
    /// a single declaration file. `false` when there was nothing there.
    fn discard(path: &Path) -> Result<bool> {
        if !path.exists() {
            return Ok(false);
        }
        match path.is_dir() {
            true => crate::fs::remove_dir_all(path)?,
            false => crate::fs::remove_file(path)?,
        }
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::module::Packages;

    /// A mirror against a throwaway project and package directory.
    fn mirror<'a>(project: &'a Config, dir: &'a Path) -> Mirror<'a> {
        Mirror::new(project, Some(dir))
    }

    fn project(root: &Path) -> Config {
        Config {
            root: root.to_path_buf(),
            ..Config::default()
        }
    }

    /// The point of mirroring the typst half is that *typst's* resolver finds
    /// it, so the assertion goes through typst-kit's own package lookup rather
    /// than checking for files by path: a layout that satisfies this is a
    /// layout an editor resolves.
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

    /// Every family reports itself the same way, so the command prints one
    /// shape and a family added later needs no special case.
    #[test]
    fn every_family_reports_where_it_wrote_and_what_is_in_it() {
        let dir = tempfile::tempdir().unwrap();
        let root = tempfile::tempdir().unwrap();
        let config = project(root.path());

        let written = mirror(&config, dir.path()).install().expect("install");

        assert_eq!(written.len(), builtin().len());
        for family in &written {
            assert!(family.path.exists(), "{} wrote nothing", family.label);
            assert!(!family.modules.is_empty(), "{} named nothing", family.label);
        }
    }

    /// Uninstalling takes what the run owns and nothing around it: a package
    /// directory is shared with a reader's own `@local` packages, and taking
    /// the lot would be a very bad way to learn that.
    #[test]
    fn uninstalling_leaves_everything_it_does_not_own_alone() {
        let dir = tempfile::tempdir().unwrap();
        let root = tempfile::tempdir().unwrap();
        let config = project(root.path());
        let neighbour = dir.path().join("local").join("theme").join("0.1.0");
        std::fs::create_dir_all(&neighbour).unwrap();
        mirror(&config, dir.path()).install().expect("install");

        let removed = mirror(&config, dir.path()).uninstall().expect("uninstall");

        assert!(removed.iter().all(|family| family.path.is_some()));
        assert!(!Packages::namespace(dir.path()).exists());
        assert!(neighbour.exists());
        // A second run is not an error: there is simply nothing there.
        let again = mirror(&config, dir.path()).uninstall().expect("uninstall");
        assert!(again.iter().all(|family| family.path.is_none()));
    }

    /// A project that has never been built still mirrors every module, with the
    /// table ones empty rather than missing, so an editor resolves the import
    /// either way and hears why the values are absent.
    #[test]
    fn the_tables_mirror_empty_before_a_first_build() {
        let dir = tempfile::tempdir().unwrap();
        let root = tempfile::tempdir().unwrap();
        let config = project(root.path());

        let written = mirror(&config, dir.path()).install().expect("install");

        let typst = written.first().expect("the typst family is first");
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
