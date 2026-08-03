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

use std::fmt::Display;
use std::path::{Path, PathBuf};

use miette::Diagnostic;
use owo_colors::OwoColorize;

use crate::config::Config;
use crate::error::Result;
use crate::generated::Generated;
use crate::ui::{Count, Level, Paths, Ui};

/// One thing a reader has to *know* once a family is mirrored, as a typed
/// advice diagnostic: rendered with the warnings, counted against nothing.
/// What a reader has to *do* is a [`Setup`] instead.
type Advice = Box<dyn Diagnostic + Send + Sync>;

/// One family of generated modules, mirrored where an editor resolves it.
///
/// Adding a family is one impl and one line in [`builtin`]. A family compiled
/// out is a row that is not there, which is why the `js` feature is spelled
/// here rather than at the call sites.
trait Target {
    /// What one of the family's modules is called in the command's own output,
    /// singular: the run counts them and [`Count`] does the plural, so no
    /// family spells its own.
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
    /// The settings this family still needs to resolve in an editor.
    setup: Vec<Setup>,
    /// What is true of the mirror and is nobody's fault: what is empty until a
    /// first build.
    notes: Vec<Advice>,
}

/// One setting a reader has to make for a mirrored family to resolve, and the
/// tool that reads it.
///
/// Deliberately not a diagnostic. These are the *point* of the command, not
/// something that went wrong with it, and a successful run whose last word is a
/// block of `☞` advice reads as one that half-failed. They print as the run's
/// own result, in one aligned block, at the bottom where a reader looks for
/// what to do next.
struct Setup {
    /// The tool the setting belongs to, as the arrow's label.
    tool: &'static str,
    /// The setting itself, ready to paste.
    value: String,
    /// Where else the same setting goes, dimmed under it. `None` when there is
    /// only one place for it.
    hint: Option<&'static str>,
}

/// A path as the reader would type it: relative to the project when it is
/// inside it, absolute when it is not (`--global`, or a `--path` elsewhere).
/// Both reports render their paths through this, so neither prints a
/// forty-column absolute path for a directory two levels down.
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

/// What one target wrote.
struct Written {
    label: &'static str,
    path: PathBuf,
    /// One row per module, listed under the line when asked for.
    modules: Vec<String>,
    notes: Vec<Advice>,
}

/// What a whole run wrote, and the shape it prints in.
///
/// The printing lives here rather than in the CLI because it is one shape for
/// every caller: `mirror` prints it, `init` prints it, and a family added later
/// is one more row in it.
pub struct Install {
    families: Vec<Written>,
    /// The settings the run still needs, gathered across the families so they
    /// print as one block rather than one per family.
    setup: Vec<Setup>,
    /// The project the paths are reported relative to.
    root: PathBuf,
}

impl Install {
    /// One result line per family: how many modules landed, and where.
    ///
    /// The module names come only with `-v`. There are sixteen of them, they
    /// are the same sixteen every time, and a reader whose next move is in
    /// [`setup`](Self::setup) had to scroll past all of them to reach it.
    ///
    /// Takes the run by value because a note is a boxed diagnostic that the
    /// [`Ui`] takes ownership of; the settings are read back off the returned
    /// [`Settings`], which is what a caller wants anyway (`init` prints them
    /// half a project later).
    pub fn render(self, ui: &Ui) -> Settings {
        // Padded so the paths line up in a column: the count is what differs
        // between the rows, and an unaligned second column reads as two
        // unrelated lines rather than as a table of two.
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
/// they belong: `mirror` right under its result, `init` in its closing block,
/// half a project later, where a reader is already looking for the next step.
pub struct Settings(Vec<Setup>);

impl Settings {
    /// Print them as one aligned block under a heading. Nothing at all when
    /// there is nothing to set, which is what `--global` buys.
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

/// What a whole run removed. Nothing to remove is not an error: a second
/// uninstall is a no-op, and says so.
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

/// What one target removed.
struct Removed {
    label: &'static str,
    path: Option<PathBuf>,
}

impl Removed {
    /// The family named as a group, since an uninstall takes all of them or
    /// none: there is no count to agree with here.
    fn plural(&self) -> String {
        format!("{}s", self.label)
    }
}

/// One mirroring run: the project it mirrors, and where its targets write.
pub struct Mirror<'a> {
    /// The project whose data the modules are generated from, and whose `root`
    /// a project-local target writes under.
    config: &'a Config,
    /// `--path`: where the packages go instead of the default.
    dir: Option<&'a Path>,
    /// `--global`: put the packages in typst's own package directory, where
    /// they resolve with nothing configured and every project shares one copy.
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

    /// Write every family, reporting what landed where.
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
        Mirror::new(project, Some(dir), false)
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

        assert_eq!(written.families.len(), builtin().len());
        for family in &written.families {
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

        assert!(removed.families.iter().all(|f| f.path.is_some()));
        assert!(!Packages::namespace(dir.path()).exists());
        assert!(neighbour.exists());
        // A second run is not an error: there is simply nothing there.
        let again = mirror(&config, dir.path()).uninstall().expect("uninstall");
        assert!(again.families.iter().all(|f| f.path.is_none()));
    }

    /// The reason the packages default into the project: three of the four
    /// typst modules describe *this* site, so one machine-global copy would
    /// show one project's pages to every other project's editor.
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

    /// A project that has never been built still mirrors every module, with the
    /// table ones empty rather than missing, so an editor resolves the import
    /// either way and hears why the values are absent.
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
