//! The fonts a compile may reach, and the order they are searched in.

use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use typst::text::{Font, FontBook};
use typst::utils::LazyHash;
use typst_kit::fonts::FontStore;

use crate::config::FontConfig;
use crate::graph::Hash;

/// The initializer a [`Fonts`] defers. Boxed because it captures the site's own
/// directories, which a plain `fn()` cannot.
type Discover = Box<dyn FnOnce() -> FontStore + Send + Sync>;

/// The files under a site's own font directories, by path, each with the hash of
/// its contents (`None` for one that could not be read). Ordered, because it is
/// hashed: a set would fingerprint the same directory differently each build.
type Faces = std::collections::BTreeMap<PathBuf, Option<Hash>>;

/// Every face a compile can resolve a glyph to, discovered on first lookup.
///
/// Lazy because scanning font directories and parsing fontconfig is not cheap
/// and a fully-cached rebuild compiles nothing: a site whose pages all hit the
/// cache never pays for a single directory walk.
pub(super) struct Fonts {
    store: LazyLock<FontStore, Discover>,
    /// The site's own directories, resolved against the root.
    ///
    /// Kept beside the store rather than only inside its initializer so
    /// [`Fonts::digest`] can fingerprint what they hold without forcing the
    /// scan, which is the whole point of the laziness above.
    dirs: Vec<PathBuf>,
}

impl Fonts {
    /// The store this site asks for, resolved against `root`, with nothing
    /// scanned yet.
    ///
    /// Search order is the whole of what this decides, and it runs from most
    /// specific to least: typst's own bundled faces, then the directories the
    /// site ships, then the machine's.
    ///
    /// The bundled faces lead so a glyph resolves the way it does under `typst`
    /// itself rather than falling back to whatever the system offers (which can
    /// rasterize digits as colour-font images). A site's own directories come
    /// next, ahead of the machine's, because shipping a face is how a site says
    /// it means *that* one and not a same-named installed version of it.
    pub(super) fn of(config: &FontConfig, root: &Path) -> Self {
        let dirs: Vec<PathBuf> = config.paths.iter().map(|dir| root.join(dir)).collect();
        let paths = dirs.clone();
        let system = config.system;
        Self {
            dirs,
            store: LazyLock::new(Box::new(move || {
                let mut fonts = FontStore::new();
                // Without the `embedded-fonts` feature the defaults are not
                // bundled, so resolution depends entirely on the other two
                // sources.
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

    /// A fingerprint of the faces the site itself ships, or `None` when it ships
    /// none.
    ///
    /// A build dependency the tracker cannot see. A face is resolved by *name*
    /// out of a store built by walking a directory, never opened through
    /// [`World::file`](typst::World::file), so no page ever records reading one.
    /// Naming a directory is in the config hash, but replacing a file inside one
    /// already named was invisible: every page hit the cache, and the social
    /// cards and PDFs typeset with the old face were served out of a green
    /// build, indefinitely.
    ///
    /// Content, not mtime: a checkout gives every file the same fresh timestamp,
    /// which would cold-rebuild every CI run while still missing a face restored
    /// from a backup. The walk is the site's own directory, and blake3 over a few
    /// megabytes of faces is noise beside one page compile.
    ///
    /// The other two sources need nothing. Typst's bundled faces are pinned by
    /// the `typst` version, which [`Renderer`](crate::graph::Renderer) already
    /// carries, and the machine's are not part of the project: a site that wants
    /// its build to depend on nothing outside the repo writes `system #false`.
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

    /// Every file under `dir`, recursively, with the hash of its contents.
    ///
    /// A directory that cannot be read contributes nothing rather than failing:
    /// one that is not there is already refused by
    /// [`FontConfig::missing`](crate::config::FontConfig::missing) before
    /// anything compiles, and a build must not die over a permission on a
    /// subdirectory it may not even have wanted.
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

    /// A site that ships no faces pays nothing: there is no directory to walk,
    /// and no digest to fold into the fingerprint every build.
    #[test]
    fn a_site_shipping_no_faces_has_no_digest() {
        let root = tempfile::tempdir().expect("tempdir");
        assert!(fonts(root.path(), &[]).digest().is_none());
    }

    /// The bug this exists for: the directory is named in the config either way,
    /// so replacing a face inside it changed nothing the cache could see, and
    /// every page (with its cards and PDFs, typeset with the old face) stayed a
    /// hit.
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

        // A face in a subdirectory counts too: the scan recurses, so the digest
        // must as well or half the tree is unwatched.
        std::fs::write(dir.join("italic/slanted.ttf"), b"three").expect("write");
        assert_ne!(after, fonts(root.path(), &["faces"]).digest());
    }

    /// Same bytes, same digest: an unchanged directory must not cold-rebuild the
    /// site on every build.
    #[test]
    fn an_unchanged_directory_digests_the_same() {
        let root = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir_all(root.path().join("faces")).expect("mkdir");
        std::fs::write(root.path().join("faces/a.ttf"), b"bytes").expect("write");

        let once = fonts(root.path(), &["faces"]).digest();
        assert_eq!(once, fonts(root.path(), &["faces"]).digest());
    }
}
