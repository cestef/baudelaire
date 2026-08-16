//! The themes this binary carries, and the source that installs them.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use super::source::{Fetched, Origin, Source};
use crate::error::{Result, ThemeError};

/// A theme shipped inside the binary, ready to be written into a project.
pub struct Bundled {
    /// What `baudelaire theme add` accepts, and the directory the copy lands in.
    pub name: &'static str,
    /// The kind of site it is for, one line, as `theme list` prints it.
    pub about: &'static str,
    files: include_dir::Dir<'static>,
}

/// The themes this binary carries: the names `theme add` resolves, the rows
/// `theme list` prints, and the suggestion a missing `--theme` directory closes
/// on all read this one table.
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

    /// Whether a `--theme` spec names one of these, taking its last segment as
    /// the name a copy would be known by.
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

    /// Every file in the theme, however deep: `include_dir` walks one level at
    /// a time.
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
    /// A config line naming this theme decides; `themes/<name>` is only the
    /// default `theme add` writes to.
    pub fn dir(&self, configured: Option<&str>) -> PathBuf {
        Self::directory(self.name, configured)
    }

    /// [`Bundled::dir`] for a theme this binary does not carry.
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

    /// A bare name, and only one the binary carries: an unrecognised word has
    /// to fall through to the sources that fetch rather than be rejected here.
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

    fn fetch(&self, origin: &Origin, _cx: &super::source::Fetching) -> Result<Fetched> {
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
            assert!(fetched.install(&dir).expect("reinstall").is_empty());
        }
    }

    #[test]
    fn the_shelf_claims_a_name_it_carries_and_no_other() {
        use super::Shelf;
        use crate::theme::Source;

        assert!(Shelf.parse("albatros").is_some());
        assert!(Shelf.parse("plume").is_none());
        assert!(Shelf.parse("themes/albatros").is_none());
    }

    #[test]
    fn a_theme_is_looked_for_where_the_config_names_it() {
        let theme = Bundled::find("albatros").expect("shipped");
        let default = PathBuf::from("themes/albatros");
        assert_eq!(theme.dir(None), default);
        assert_eq!(
            theme.dir(Some("vendor/albatros")),
            PathBuf::from("vendor/albatros")
        );
        assert_eq!(theme.dir(Some("vendor/spleen")), default);
        assert_eq!(theme.dir(Some("@local/albatros:0.1.0")), default);
    }

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

    #[test]
    fn an_unknown_theme_suggests_a_shipped_one() {
        let Err(err) = Bundled::find("albatross") else {
            panic!("not a shipped theme");
        };
        let rendered = format!("{err:?}");
        assert!(rendered.contains("albatros"), "{rendered}");
    }
}
