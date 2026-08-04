//! The themes this binary carries, and the source that installs them.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use super::source::{Fetched, Origin, Source};
use crate::error::{Result, ThemeError};

/// A theme shipped inside the binary, ready to be written into a project.
///
/// A theme is files, and the four the project ships are small enough to carry
/// (about 60 KiB each) that "where do I get one" stops being a step. Every
/// other source has to reach for one; this one is the answer to "I have not
/// chosen yet", which is why it is offered a spec first and why it needs no
/// network.
///
/// Whether a copied theme stays in step with the binary is the project's to
/// decide, as it is for the scaffolded templates: the copy is yours the moment
/// it lands.
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

    /// This theme's files, as any source hands them over.
    pub fn fetched(&self) -> Fetched {
        Fetched {
            name: self.name.to_owned(),
            about: Some(self.about.to_owned()),
            origin: Origin::Bundled {
                name: self.name.to_owned(),
            },
            files: Self::walk(&self.files)
                .into_iter()
                .map(|file| (file.path().to_path_buf(), file.contents().to_vec()))
                .collect::<BTreeMap<PathBuf, Vec<u8>>>(),
        }
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

    /// The directory a project keeps this theme in, given the `theme` line its
    /// config carries.
    ///
    /// The config is the one place that says where a theme actually is, so a
    /// line naming this theme decides, and `themes/<name>` is only the default
    /// `theme add` writes to. The single answer every verb reads: without it
    /// the shelf reported nothing for a theme installed anywhere else, while
    /// `info` and `update`, which take `--dir`, were looking straight at it.
    pub fn dir(&self, configured: Option<&str>) -> PathBuf {
        Self::directory(self.name, configured)
    }

    /// [`Bundled::dir`] for a theme this binary does not carry: the name is all
    /// it takes, so a theme fetched from anywhere is found the same way.
    pub fn directory(name: &str, configured: Option<&str>) -> PathBuf {
        configured
            .filter(|spec| spec.rsplit('/').next() == Some(name))
            .map_or_else(|| Path::new(Self::DIR).join(name), PathBuf::from)
    }

    /// Where `theme add` writes a copy, and the directory a `theme` line names
    /// when nothing says otherwise.
    pub const DIR: &'static str = "themes";
}

/// The shelf: the themes inside the binary, named by a bare word.
pub struct Shelf;

impl Source for Shelf {
    fn name(&self) -> &'static str {
        "bundled"
    }

    /// A bare name, and only one the binary actually carries: every other
    /// source recognises a spec by its shape (a scheme, a path, a `@`), so an
    /// unrecognised word has to fall through to them rather than be claimed
    /// here and rejected.
    fn parse(&self, spec: &str) -> Option<Origin> {
        BUNDLED
            .iter()
            .find(|theme| theme.name == spec)
            .map(|theme| Origin::Bundled {
                name: theme.name.to_owned(),
            })
    }

    fn owns(&self, origin: &Origin) -> bool {
        matches!(origin, Origin::Bundled { .. })
    }

    fn fetch(&self, origin: &Origin) -> Result<Fetched> {
        let Origin::Bundled { name } = origin else {
            return Err(ThemeError::unsupported(origin.label()).into());
        };
        Ok(Bundled::find(name)?.fetched())
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::{BUNDLED, Bundled};

    /// Every shipped theme is complete enough to be a theme: its defaults and
    /// at least one layout, so `theme add` cannot write a directory that fails
    /// on the first build.
    #[test]
    fn every_shipped_theme_carries_its_config_and_templates() {
        let tmp = tempfile::tempdir().expect("tempdir");
        for theme in BUNDLED {
            let dir = tmp.path().join(theme.name);
            let fetched = theme.fetched();
            let written = fetched.install(&dir).expect("install");
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
            assert!(fetched.install(&dir).expect("reinstall").is_empty());
        }
    }

    /// A bare word is the shelf's, and only when the binary carries it: an
    /// unknown one has to fall through to the sources that fetch, or a typo
    /// would be answered by whichever of them claims a bare word.
    #[test]
    fn the_shelf_claims_a_name_it_carries_and_no_other() {
        use super::Shelf;
        use crate::theme::Source;

        assert!(Shelf.parse("albatros").is_some());
        assert!(Shelf.parse("plume").is_none());
        assert!(Shelf.parse("themes/albatros").is_none());
    }

    /// The config is what says where a theme is; `themes/<name>` is only the
    /// default `add` writes to. A `list` that read the default alone reported
    /// nothing for a theme kept anywhere else.
    #[test]
    fn a_theme_is_looked_for_where_the_config_names_it() {
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
