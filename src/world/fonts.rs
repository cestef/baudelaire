//! The fonts a compile may reach, and the order they are searched in.

use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use typst::text::{Font, FontBook};
use typst::utils::LazyHash;
use typst_kit::fonts::FontStore;

use crate::config::FontConfig;
use crate::graph::Hash;

/// The initializer a [`Fonts`] defers.
type Discover = Box<dyn FnOnce() -> FontStore + Send + Sync>;

/// The files under a site's own font directories, each with the hash of its
/// contents (`None` for one that could not be read), ordered because it is
/// hashed.
type Faces = std::collections::BTreeMap<PathBuf, Option<Hash>>;

/// Every face a compile can resolve a glyph to, discovered on first lookup so
/// a fully-cached rebuild never walks a font directory.
pub(super) struct Fonts {
    store: LazyLock<FontStore, Discover>,
    /// The site's own directories, resolved against the root, kept out of the
    /// initializer so [`Fonts::digest`] can read them without forcing the scan.
    dirs: Vec<PathBuf>,
}

impl Fonts {
    /// The store this site asks for, resolved against `root`, with nothing
    /// scanned yet.
    ///
    /// Searched most specific first: typst's bundled faces, then the ones the
    /// site ships, then the machine's.
    pub(super) fn of(config: &FontConfig, root: &Path) -> Self {
        let dirs: Vec<PathBuf> = config.paths.iter().map(|dir| root.join(dir)).collect();
        let paths = dirs.clone();
        let system = config.system;
        Self {
            dirs,
            store: LazyLock::new(Box::new(move || {
                let mut fonts = FontStore::new();
                // Without this feature the defaults are not bundled at all.
                #[cfg(feature = "embedded-fonts")]
                fonts.extend(typst_kit::fonts::embedded());
                for dir in &paths {
                    fonts.extend(typst_kit::fonts::scan(dir));
                }
                if system {
                    fonts.extend(typst_kit::fonts::system());
                }
                fonts
            })),
        }
    }

    /// A fingerprint of the faces the site itself ships, or `None` when it
    /// ships none.
    ///
    /// A face is resolved by *name* out of a directory walk, never opened
    /// through [`World::file`](typst::World::file), so no page records reading
    /// one and nothing else invalidates a build when a face is replaced.
    pub(super) fn digest(&self) -> Option<Hash> {
        if self.dirs.is_empty() {
            return None;
        }
        let mut faces = Faces::new();
        for dir in &self.dirs {
            Self::walk(dir, &mut faces);
        }
        Some(Hash::of(&faces))
    }

    /// Every file under `dir`, recursively, with the hash of its contents; a
    /// directory that cannot be read contributes nothing rather than failing.
    fn walk(dir: &Path, into: &mut Faces) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                Self::walk(&path, into);
                continue;
            }
            let digest = Hash::of_file(&path);
            into.insert(path, digest);
        }
    }

    pub(super) fn book(&self) -> &LazyHash<FontBook> {
        self.store.book()
    }

    pub(super) fn font(&self, index: usize) -> Option<Font> {
        self.store.font(index)
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::Fonts;
    use crate::config::FontConfig;

    fn fonts(root: &std::path::Path, dirs: &[&str]) -> Fonts {
        let config = FontConfig {
            paths: dirs.iter().map(PathBuf::from).collect(),
            system: false,
        };
        Fonts::of(&config, root)
    }

    #[test]
    fn a_site_shipping_no_faces_has_no_digest() {
        let root = tempfile::tempdir().expect("tempdir");
        assert!(fonts(root.path(), &[]).digest().is_none());
    }

    #[test]
    fn replacing_a_face_changes_the_digest() {
        let root = tempfile::tempdir().expect("tempdir");
        let dir = root.path().join("faces");
        std::fs::create_dir_all(dir.join("italic")).expect("mkdir");
        std::fs::write(dir.join("regular.ttf"), b"one").expect("write");

        let before = fonts(root.path(), &["faces"]).digest();
        assert!(before.is_some());

        std::fs::write(dir.join("regular.ttf"), b"two").expect("write");
        let after = fonts(root.path(), &["faces"]).digest();
        assert_ne!(before, after, "a face's contents are the fingerprint");

        std::fs::write(dir.join("italic/slanted.ttf"), b"three").expect("write");
        assert_ne!(after, fonts(root.path(), &["faces"]).digest());
    }

    #[test]
    fn an_unchanged_directory_digests_the_same() {
        let root = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir_all(root.path().join("faces")).expect("mkdir");
        std::fs::write(root.path().join("faces/a.ttf"), b"bytes").expect("write");

        let once = fonts(root.path(), &["faces"]).digest();
        assert_eq!(once, fonts(root.path(), &["faces"]).digest());
    }
}
