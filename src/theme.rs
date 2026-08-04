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
    const TEMPLATES: &'static str = "templates";
    const ASSETS: &'static str = "assets";
    const STATIC: &'static str = "static";
    const CONFIG: &'static str = "theme.kdl";

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

/// What one file of an installed theme is, compared against the copy that was
/// written: the vocabulary `update` and `remove` act on, and `list` reports.
#[cfg(feature = "themes")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    /// Byte-identical to what was installed: safe to replace or delete.
    Pristine,
    /// Present and changed since it was installed: the author's, not ours.
    Edited,
    /// Recorded as installed and no longer on disk: deleted deliberately, so
    /// neither `update` nor `remove` puts it back.
    Gone,
    /// In the shipped theme and absent from this copy: a file this version adds.
    Added,
    /// In the shipped theme, here, and never ours: an install found it and
    /// skipped it. The author's whatever it holds, so `update` replaces it only
    /// under `force` and `remove` never deletes it: taking our copy off is no
    /// licence to delete a file we did not write.
    Yours,
}

/// One file of an installed theme, with what became of it.
#[cfg(feature = "themes")]
pub struct Tracked {
    /// Path relative to the theme directory.
    pub rel: PathBuf,
    pub state: State,
}

/// The record `theme add` leaves inside an installed theme: which theme it is,
/// which baudelaire wrote it, and what each of its files is in that baudelaire.
///
/// This is the whole of baudelaire's package state, and it is deliberately
/// small. A theme is *vendored*: its files are in the project, committed with
/// it, and yours to edit, so the thing worth recording is not a version to
/// resolve later but which bytes were ours. That is what lets `update` replace
/// the files you have not touched and keep the ones you have, and what lets
/// `remove` refuse to delete work.
///
/// It travels inside the theme directory, beside the files it describes, the
/// way a vendored crate carries its own checksums.
#[cfg(feature = "themes")]
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct Lock {
    /// The shipped theme this copy came from.
    pub theme: String,
    /// The baudelaire that wrote it, so `update` can say what it is updating
    /// from.
    pub baudelaire: String,
    /// Relative path to the digest baudelaire's own copy of the file has.
    files: std::collections::BTreeMap<String, String>,
}

#[cfg(feature = "themes")]
impl Lock {
    /// The file the record lives in, inside the theme directory. Hidden and
    /// namespaced: it is baudelaire's bookkeeping, not part of the theme.
    pub const FILE: &'static str = ".baudelaire-lock.json";

    /// The record inside `dir`, or `None` for a theme nothing installed: one
    /// written by hand, or a copy from before this existed. Everything reading
    /// it treats that as "every file is the author's".
    pub fn read(dir: &Path) -> Option<Self> {
        serde_json::from_slice(&std::fs::read(dir.join(Self::FILE)).ok()?).ok()
    }

    /// Whether `dir` holds a copy this wrote.
    pub fn installed(dir: &Path) -> bool {
        dir.join(Self::FILE).is_file()
    }

    /// What each file of the installed copy is now. Ordered by path, so a
    /// report reads the same way twice.
    pub fn state(
        &self,
        dir: &Path,
        shipped: &[&'static include_dir::File<'static>],
    ) -> Vec<Tracked> {
        let mut tracked: Vec<Tracked> = self
            .files
            .iter()
            .map(|(rel, digest)| Tracked {
                rel: PathBuf::from(rel),
                state: match std::fs::read(dir.join(rel)) {
                    Err(_) => State::Gone,
                    Ok(bytes) if crate::graph::Hash::of_bytes(&bytes).hex() == *digest => {
                        State::Pristine
                    }
                    Ok(_) => State::Edited,
                },
            })
            .collect();
        // A shipped file the record does not claim: one this version gained, or
        // one that was already here when the install ran and was skipped. On
        // disk tells the two apart, and they are not the same file at all: the
        // first is ours to write, the second is the author's to keep.
        tracked.extend(
            shipped
                .iter()
                .map(|file| file.path())
                .filter(|rel| !self.files.contains_key(&rel.to_string_lossy().into_owned()))
                .map(|rel| Tracked {
                    rel: rel.to_path_buf(),
                    state: match dir.join(rel).exists() {
                        true => State::Yours,
                        false => State::Added,
                    },
                }),
        );
        tracked.sort_by(|a, b| a.rel.cmp(&b.rel));
        tracked
    }

    /// Record the files a run just wrote as ours, on top of what an earlier one
    /// left, digesting the bytes this binary carries.
    ///
    /// Never what is on disk. Digesting the disk would record a file kept
    /// *because* it was edited as though baudelaire had written it, and the
    /// next `update` would overwrite the author's work as `Pristine`. Reading
    /// our own bytes says only what this copy of the theme is, which is the one
    /// claim the record is entitled to make: a file that differs from it is the
    /// author's, whether they changed it after an install or had it before one.
    ///
    /// And only what was *written*. A file already there was skipped, so this
    /// run learned nothing about it: an entry an earlier install left stands, at
    /// the digest that install wrote, and a file that was never ours gains no
    /// entry at all. Re-recording the whole shipped set was how a second `add`
    /// broke `update`: every file an older baudelaire had written came back with
    /// *this* binary's digest, so an untouched file read as `Edited` and update
    /// kept it, release after release.
    fn claim(
        dir: &Path,
        theme: &str,
        written: &[&'static include_dir::File<'static>],
    ) -> Result<()> {
        let previous = Self::read(dir).filter(|lock| lock.theme == theme);
        // Nothing of ours here and nothing written: a directory that is entirely
        // the author's stays that way rather than gaining a record of no files.
        if written.is_empty() && previous.is_none() {
            return Ok(());
        }
        let mut files = previous
            .as_ref()
            .map_or_else(std::collections::BTreeMap::new, |lock| lock.files.clone());
        files.extend(written.iter().map(|file| {
            (
                file.path().to_string_lossy().into_owned(),
                crate::graph::Hash::of_bytes(file.contents()).hex(),
            )
        }));
        let lock = Self {
            theme: theme.to_owned(),
            // What wrote the files that are here. A run that wrote none leaves
            // the record describing the baudelaire that did.
            baudelaire: match written.is_empty() {
                true => previous.map_or_else(|| crate::VERSION.to_owned(), |lock| lock.baudelaire),
                false => crate::VERSION.to_owned(),
            },
            files,
        };
        let json = serde_json::to_vec_pretty(&lock)
            .map_err(|why| ThemeError::lock(dir.join(Self::FILE).display(), why))?;
        crate::fs::write(dir.join(Self::FILE), json)
    }
}

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

    /// The directory a project keeps this theme in, given the `theme` line its
    /// config carries.
    ///
    /// The config is the one place that says where a theme actually is, so a
    /// line naming this theme decides, and `themes/<name>` is only the default
    /// `theme add` writes to. The single answer every verb reads: without it
    /// the shelf reported nothing for a theme installed anywhere else, while
    /// `info` and `update`, which take `--dir`, were looking straight at it.
    pub fn dir(&self, configured: Option<&str>) -> PathBuf {
        configured
            .filter(|spec| Self::named_by(spec).is_some_and(|named| named.name == self.name))
            .map_or_else(|| Path::new(Self::DIR).join(self.name), PathBuf::from)
    }

    /// Where `theme add` writes a copy, and the directory a `theme` line names
    /// when nothing says otherwise.
    pub const DIR: &'static str = "themes";

    /// Write the theme's files under `dir`, skipping any that are already
    /// there, and report what was written. An existing file is the author's:
    /// a second `theme add` over an edited copy must not silently undo it.
    ///
    /// The copy is recorded in a [`Lock`] beside the files, which is what later
    /// tells your edits from ours.
    pub fn install(&self, dir: &Path) -> Result<Vec<PathBuf>> {
        let mut written = Vec::new();
        for file in self.shipped() {
            let dst = dir.join(file.path());
            if dst.exists() {
                continue;
            }
            Self::place(&dst, file.contents())?;
            written.push(file);
        }
        Lock::claim(dir, self.name, &written)?;
        let mut paths: Vec<PathBuf> = written
            .iter()
            .map(|file| file.path().to_path_buf())
            .collect();
        paths.sort();
        Ok(paths)
    }

    /// Bring an installed copy up to this binary's version, file by file.
    ///
    /// Only what the [`Lock`] says is still ours is replaced: a file you have
    /// edited is left alone and reported, one you deleted stays deleted, and
    /// one this version adds is written. `force` replaces the edited ones too,
    /// which is the only way to get back to a clean copy.
    ///
    /// A copy with no lock (installed by hand, or by a baudelaire from before
    /// this) is entirely yours: nothing is replaced without `force`.
    pub fn update(&self, dir: &Path, force: bool) -> Result<Vec<Tracked>> {
        if !dir.is_dir() {
            return Err(ThemeError::not_installed(&dir.display().to_string()).into());
        }
        let shipped = self.shipped();
        let tracked = match Lock::read(dir) {
            Some(lock) => lock.state(dir, &shipped),
            None => shipped
                .iter()
                .map(|file| Tracked {
                    rel: file.path().to_path_buf(),
                    state: State::Edited,
                })
                .collect(),
        };
        let mut written = Vec::new();
        for file in &shipped {
            let rel = file.path();
            let state = tracked
                .iter()
                .find(|t| t.rel == rel)
                .map_or(State::Added, |t| t.state);
            let replace = match state {
                State::Pristine | State::Added => true,
                State::Edited | State::Yours => force,
                // Deleted on purpose: an update that puts a file back is an
                // update that undoes a decision.
                State::Gone => false,
            };
            if replace {
                Self::place(&dir.join(rel), file.contents())?;
                written.push(*file);
            }
        }
        Lock::claim(dir, self.name, &written)?;
        Ok(tracked)
    }

    /// Take an installed copy back off, and report what each of its files was.
    ///
    /// Files still ours go; anything you edited stays unless `force`, and so
    /// does anything you added, which was never ours to delete. The record goes
    /// only when nothing it tracks is left, so a `remove` that kept your edits
    /// can still be finished with `--force` later. The directory itself goes
    /// once it is empty.
    pub fn uninstall(dir: &Path, force: bool) -> Result<Vec<Tracked>> {
        let Some(lock) = Lock::read(dir) else {
            return Err(ThemeError::not_installed(&dir.display().to_string()).into());
        };
        let shipped = BUNDLED
            .iter()
            .find(|t| t.name == lock.theme)
            .map(Self::shipped)
            .unwrap_or_default();
        let tracked = lock.state(dir, &shipped);
        let mut kept = false;
        for file in &tracked {
            let remove = match file.state {
                State::Pristine => true,
                State::Edited => force,
                // `Yours` with them, at any force: taking our copy off is no
                // licence to delete a file we never wrote.
                State::Gone | State::Added | State::Yours => false,
            };
            match remove {
                true => crate::fs::remove_file(dir.join(&file.rel))?,
                false => kept |= file.state == State::Edited,
            }
        }
        if !kept {
            crate::fs::remove_file(dir.join(Lock::FILE))?;
            Self::prune(dir);
        }
        Ok(tracked)
    }

    /// Every file in the theme, however deep. `include_dir` walks one level at
    /// a time, and a theme nests (`templates/`, `assets/`, `highlight/`).
    pub fn shipped(&self) -> Vec<&'static include_dir::File<'static>> {
        let mut out = Self::walk(&self.files);
        out.sort_by_key(|file| file.path());
        out
    }

    fn walk(dir: &include_dir::Dir<'static>) -> Vec<&'static include_dir::File<'static>> {
        let mut out: Vec<&include_dir::File<'static>> = dir.files().collect();
        for sub in dir.dirs() {
            out.extend(Self::walk(sub));
        }
        out
    }

    /// Write one file, making the directories above it first.
    fn place(dst: &Path, bytes: &[u8]) -> Result<()> {
        if let Some(parent) = dst.parent() {
            crate::fs::create_dir_all(parent)?;
        }
        crate::fs::write(dst, bytes)
    }

    /// Drop the directories an uninstall emptied. [`crate::fs::Walk`] lists them
    /// children before parents, which is the order they can go in; one that
    /// still holds a file of yours simply will not, and that is the answer.
    fn prune(dir: &Path) {
        let Ok(tree) = crate::fs::Walk::new(dir).tree() else {
            return;
        };
        for path in tree.dirs.iter().chain([&dir.to_path_buf()]) {
            let _ = std::fs::remove_dir(path);
        }
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
    use super::{BUNDLED, Bundled, State};

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

    /// The whole point of the record: a file kept *because* it was edited is
    /// still the author's on the next update. Digesting the disk recorded that
    /// edit as ours, so the second update read it back as `Pristine` and
    /// overwrote it without a word.
    #[test]
    fn a_second_update_still_keeps_an_edit() {
        const MINE: &[u8] = b"/* mine */\n";
        let tmp = tempfile::tempdir().expect("tempdir");
        let dir = tmp.path().join("albatros");
        let theme = Bundled::find("albatros").expect("shipped");
        theme.install(&dir).expect("install");

        let style = dir.join("assets/style.css");
        std::fs::write(&style, MINE).expect("edit");
        for run in 1..=2 {
            theme.update(&dir, false).expect("update");
            assert_eq!(
                std::fs::read(&style).expect("read"),
                MINE,
                "update {run} took the edit"
            );
        }
    }

    /// A file already there when `theme add` ran was never ours, so the record
    /// must not claim it: an install skips it, an update leaves it alone, and a
    /// remove does not delete it even under `--force`, which is the difference
    /// between a file baudelaire wrote and one it merely found.
    #[test]
    fn an_install_does_not_claim_a_file_it_found() {
        const MINE: &[u8] = b"/* mine */\n";
        let tmp = tempfile::tempdir().expect("tempdir");
        let dir = tmp.path().join("albatros");
        std::fs::create_dir_all(dir.join("assets")).expect("mkdir");
        let style = dir.join("assets/style.css");
        std::fs::write(&style, MINE).expect("write");

        let theme = Bundled::find("albatros").expect("shipped");
        theme.install(&dir).expect("install");
        let tracked = theme.update(&dir, false).expect("update");
        assert_eq!(std::fs::read(&style).expect("read"), MINE);
        assert!(
            tracked
                .iter()
                .any(|file| file.rel == *"assets/style.css" && file.state == State::Yours),
            "a found file is the author's, not a file this version adds"
        );

        Bundled::uninstall(&dir, true).expect("remove");
        assert_eq!(std::fs::read(&style).expect("read"), MINE);
    }

    /// The bug a second `add` used to leave behind: it writes no file, so it
    /// must record no file either. Claiming the whole shipped set re-digested
    /// every file an older baudelaire had written, turning an untouched one into
    /// an `Edited` one that `update` then kept for good.
    #[test]
    fn a_stray_add_does_not_disown_an_older_install() {
        const OLD: &[u8] = b"/* the 0.0.9 stylesheet */\n";
        let rel = "assets/style.css";
        let tmp = tempfile::tempdir().expect("tempdir");
        let dir = tmp.path().join("albatros");
        let theme = Bundled::find("albatros").expect("shipped");
        theme.install(&dir).expect("install");

        // An older baudelaire's copy: an older file, and a record that says so.
        std::fs::write(dir.join(rel), OLD).expect("age");
        let mut lock = super::Lock::read(&dir).expect("lock");
        lock.files
            .insert(rel.to_owned(), crate::graph::Hash::of_bytes(OLD).hex());
        lock.baudelaire = "0.0.9".to_owned();
        std::fs::write(
            dir.join(super::Lock::FILE),
            serde_json::to_vec(&lock).expect("json"),
        )
        .expect("record");

        assert!(theme.install(&dir).expect("second add").is_empty());
        let lock = super::Lock::read(&dir).expect("lock");
        assert_eq!(
            lock.baudelaire, "0.0.9",
            "a run that wrote nothing restamped"
        );
        assert!(
            lock.state(&dir, &theme.shipped())
                .iter()
                .any(|file| file.rel == *rel && file.state == State::Pristine),
            "an untouched older file read as the author's"
        );
        theme.update(&dir, false).expect("update");
        assert_ne!(std::fs::read(dir.join(rel)).expect("read"), OLD);
    }

    /// The other half: what the record *does* claim is replaced. An older
    /// baudelaire left an older file and a record of it, and that is exactly
    /// the file an update exists to bring forward.
    #[test]
    fn an_update_rewrites_a_file_the_record_still_claims() {
        const OLD: &[u8] = b"// an older page.typ\n";
        let tmp = tempfile::tempdir().expect("tempdir");
        let dir = tmp.path().join("albatros");
        let theme = Bundled::find("albatros").expect("shipped");
        theme.install(&dir).expect("install");

        let rel = "templates/page.typ";
        std::fs::write(dir.join(rel), OLD).expect("age");
        let mut lock = super::Lock::read(&dir).expect("lock");
        lock.files
            .insert(rel.to_owned(), crate::graph::Hash::of_bytes(OLD).hex());
        std::fs::write(
            dir.join(super::Lock::FILE),
            serde_json::to_vec(&lock).expect("json"),
        )
        .expect("record");

        theme.update(&dir, false).expect("update");
        assert_ne!(std::fs::read(dir.join(rel)).expect("read"), OLD);
    }

    /// The config is what says where a theme is; `themes/<name>` is only the
    /// default `add` writes to. A `list` that read the default alone reported
    /// nothing for a theme kept anywhere else.
    #[test]
    fn a_theme_is_looked_for_where_the_config_names_it() {
        use std::path::PathBuf;

        let theme = Bundled::find("albatros").expect("shipped");
        let default = PathBuf::from("themes/albatros");
        assert_eq!(theme.dir(None), default);
        assert_eq!(
            theme.dir(Some("vendor/albatros")),
            PathBuf::from("vendor/albatros")
        );
        // A line naming another theme, or a package rather than a directory,
        // says nothing about where this one would be.
        assert_eq!(theme.dir(Some("vendor/spleen")), default);
        assert_eq!(theme.dir(Some("@local/albatros:0.1.0")), default);
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
