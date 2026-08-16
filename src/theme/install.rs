//! Vendoring a fetched theme: writing it into the project, recording which
//! bytes were ours, and telling the two apart ever after.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::source::{Fetched, Origin};
use crate::error::{Result, ThemeError};
use crate::graph::Hash;

/// What one file of an installed theme is, compared against the copy that was
/// written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    /// Byte-identical to what was installed: safe to replace or delete.
    Pristine,
    /// Present and changed since it was installed: the author's, not ours.
    Edited,
    /// Recorded as installed and no longer on disk, so neither `update` nor
    /// `remove` puts it back.
    Gone,
    /// In the fetched theme and absent from this copy: a file this version
    /// adds.
    Added,
    /// In the fetched theme, here, and never ours: `update` replaces it only
    /// under `force` and `remove` never deletes it.
    Yours,
}

pub struct Tracked {
    /// Relative to the theme directory.
    pub rel: PathBuf,
    pub state: State,
}

/// The record an install leaves inside a theme directory, answering which of
/// these files are still ours: what lets `update` replace the ones you have not
/// touched and `remove` refuse to delete work.
#[derive(Debug, Serialize, Deserialize)]
pub struct Lock {
    pub theme: String,
    /// Absent in a record written before a theme could come from anywhere but
    /// the binary: see [`Lock::origin`].
    origin: Option<Origin>,
    pub baudelaire: String,
    /// Relative path to the digest baudelaire's own copy of the file has.
    files: BTreeMap<String, String>,
}

impl Lock {
    pub const FILE: &'static str = ".baudelaire-lock.json";

    /// `None` for a theme nothing installed, which every reader treats as
    /// "every file is the author's".
    pub fn read(dir: &Path) -> Option<Self> {
        serde_json::from_slice(&std::fs::read(dir.join(Self::FILE)).ok()?).ok()
    }

    pub fn installed(dir: &Path) -> bool {
        dir.join(Self::FILE).is_file()
    }

    /// A record with no origin was written when the only source was the binary,
    /// and named that shipped theme in `theme`.
    pub fn origin(&self) -> Origin {
        self.origin.clone().unwrap_or_else(|| Origin::Bundled {
            name: self.theme.clone(),
        })
    }

    /// What each file the record claims is now, judged against the disk alone
    /// so that reporting on a copy never fetches anything.
    pub fn state(&self, dir: &Path) -> Vec<Tracked> {
        let mut tracked: Vec<Tracked> = self
            .files
            .iter()
            .map(|(rel, digest)| Tracked {
                rel: PathBuf::from(rel),
                state: match std::fs::read(dir.join(rel)) {
                    Err(_) => State::Gone,
                    Ok(bytes) if Hash::of_bytes(&bytes).hex() == *digest => State::Pristine,
                    Ok(_) => State::Edited,
                },
            })
            .collect();
        tracked.sort_by(|a, b| a.rel.cmp(&b.rel));
        tracked
    }

    fn claims(&self, rel: &Path) -> bool {
        self.files.contains_key(&rel.to_string_lossy().into_owned())
    }

    /// Files still ours go, anything edited stays unless `force`, and anything
    /// never ours stays at any force.
    ///
    /// The record goes only when nothing it tracks is left, so a `remove` that
    /// kept your edits can still be finished with `--force` later.
    pub fn uninstall(dir: &Path, force: bool) -> Result<Vec<Tracked>> {
        let Some(lock) = Self::read(dir) else {
            return Err(ThemeError::not_installed(&dir.display().to_string()).into());
        };
        let tracked = lock.state(dir);
        let mut kept = false;
        for file in &tracked {
            let remove = match file.state {
                State::Pristine => true,
                State::Edited => force,
                State::Gone | State::Added | State::Yours => false,
            };
            if remove {
                crate::fs::remove_file(dir.join(&file.rel))?;
            } else {
                kept |= file.state == State::Edited;
            }
        }
        if !kept {
            crate::fs::remove_file(dir.join(Self::FILE))?;
            Self::prune(dir);
        }
        Ok(tracked)
    }

    /// Drop the directories an uninstall emptied; [`crate::fs::Walk`] lists
    /// them children before parents, which is the order they can go in.
    fn prune(dir: &Path) {
        let Ok(tree) = crate::fs::Walk::new(dir).tree() else {
            return;
        };
        for path in tree.dirs.iter().chain([&dir.to_path_buf()]) {
            let _ = std::fs::remove_dir(path);
        }
    }

    /// The files an installed copy holds, whatever its record claims, for a
    /// report on a theme this binary cannot fetch.
    pub fn present(dir: &Path) -> BTreeSet<PathBuf> {
        crate::fs::Walk::new(dir)
            .tree()
            .map(|tree| {
                tree.files
                    .iter()
                    .filter_map(|path| path.strip_prefix(dir).ok())
                    .filter(|rel| rel != &Path::new(Self::FILE))
                    .map(Path::to_path_buf)
                    .collect()
            })
            .unwrap_or_default()
    }
}

impl Fetched {
    /// Files already there are skipped, since an existing file is the author's
    /// and a second install must not silently undo an edit.
    pub fn install(&self, dir: &Path) -> Result<Vec<PathBuf>> {
        let mut written = Vec::new();
        for (rel, contents) in &self.files {
            let dst = dir.join(rel);
            if dst.exists() {
                continue;
            }
            Self::place(&dst, contents)?;
            written.push(rel.clone());
        }
        self.claim(dir, &written)?;
        Ok(written)
    }

    /// Bring an installed copy up to this fetch, replacing only what the
    /// [`Lock`] still calls ours; `force` replaces the edited ones too, and a
    /// copy with no lock is entirely yours.
    pub fn update(&self, dir: &Path, force: bool) -> Result<Vec<Tracked>> {
        if !dir.is_dir() {
            return Err(ThemeError::not_installed(&dir.display().to_string()).into());
        }
        let tracked = self.state(dir, Lock::read(dir).as_ref());
        let mut written = Vec::new();
        for file in &tracked {
            let Some(contents) = self.files.get(&file.rel) else {
                continue;
            };
            let replace = match file.state {
                State::Pristine | State::Added => true,
                State::Edited | State::Yours => force,
                State::Gone => false,
            };
            if replace {
                Self::place(&dir.join(&file.rel), contents)?;
                written.push(file.rel.clone());
            }
        }
        self.claim(dir, &written)?;
        Ok(tracked)
    }

    /// What each of this copy's files is, the theme's own list included; being
    /// on disk is what tells a file this version adds from one already there.
    ///
    /// A copy with no record is entirely the author's, so every file of it
    /// reads as edited.
    pub fn state(&self, dir: &Path, lock: Option<&Lock>) -> Vec<Tracked> {
        let Some(lock) = lock else {
            return self
                .paths()
                .map(|rel| Tracked {
                    rel: rel.to_path_buf(),
                    state: State::Edited,
                })
                .collect();
        };
        let mut tracked = lock.state(dir);
        tracked.extend(
            self.paths()
                .filter(|rel| !lock.claims(rel))
                .map(|rel| Tracked {
                    rel: rel.to_path_buf(),
                    state: if dir.join(rel).exists() {
                        State::Yours
                    } else {
                        State::Added
                    },
                }),
        );
        tracked.sort_by(|a, b| a.rel.cmp(&b.rel));
        tracked
    }

    /// Record the files a run just wrote as ours, digesting the bytes that were
    /// fetched and never what is on disk.
    ///
    /// Digesting the disk would claim a file kept *because* it was edited, and
    /// the next `update` would overwrite the author's work as `Pristine`.
    fn claim(&self, dir: &Path, written: &[PathBuf]) -> Result<()> {
        let previous = Lock::read(dir).filter(|lock| lock.theme == self.name);
        if written.is_empty() && previous.is_none() {
            return Ok(());
        }
        let mut files = previous
            .as_ref()
            .map_or_else(BTreeMap::new, |lock| lock.files.clone());
        files.extend(written.iter().filter_map(|rel| {
            let contents = self.files.get(rel)?;
            Some((
                rel.to_string_lossy().into_owned(),
                Hash::of_bytes(contents).hex(),
            ))
        }));
        let lock = Lock {
            theme: self.name.clone(),
            origin: Some(self.origin.clone()),
            baudelaire: if written.is_empty() {
                previous.map_or_else(|| crate::VERSION.to_owned(), |lock| lock.baudelaire)
            } else {
                crate::VERSION.to_owned()
            },
            files,
        };
        let json = serde_json::to_vec_pretty(&lock)
            .map_err(|why| ThemeError::lock(dir.join(Lock::FILE).display(), why))?;
        crate::fs::write(dir.join(Lock::FILE), json)
    }

    fn place(dst: &Path, bytes: &[u8]) -> Result<()> {
        if let Some(parent) = dst.parent() {
            crate::fs::create_dir_all(parent)?;
        }
        crate::fs::write(dst, bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::{Lock, State};
    use crate::theme::Bundled;

    fn albatros() -> crate::theme::Fetched {
        Bundled::find("albatros").expect("shipped").fetched()
    }

    /// A file kept *because* it was edited is still the author's on the next
    /// update.
    #[test]
    fn a_second_update_still_keeps_an_edit() {
        const MINE: &[u8] = b"/* mine */\n";
        let tmp = tempfile::tempdir().expect("tempdir");
        let dir = tmp.path().join("albatros");
        let theme = albatros();
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

    /// A file already there when the install ran was never ours, so the record
    /// must not claim it at any force.
    #[test]
    fn an_install_does_not_claim_a_file_it_found() {
        const MINE: &[u8] = b"/* mine */\n";
        let tmp = tempfile::tempdir().expect("tempdir");
        let dir = tmp.path().join("albatros");
        std::fs::create_dir_all(dir.join("assets")).expect("mkdir");
        let style = dir.join("assets/style.css");
        std::fs::write(&style, MINE).expect("write");

        let theme = albatros();
        theme.install(&dir).expect("install");
        let tracked = theme.update(&dir, false).expect("update");
        assert_eq!(std::fs::read(&style).expect("read"), MINE);
        assert!(
            tracked
                .iter()
                .any(|file| file.rel == *"assets/style.css" && file.state == State::Yours),
            "a found file is the author's, not a file this version adds"
        );

        Lock::uninstall(&dir, true).expect("remove");
        assert_eq!(std::fs::read(&style).expect("read"), MINE);
    }

    /// A second `add` writes no file, so it must record none either.
    #[test]
    fn a_stray_add_does_not_disown_an_older_install() {
        const OLD: &[u8] = b"/* the 0.0.9 stylesheet */\n";
        let rel = "assets/style.css";
        let tmp = tempfile::tempdir().expect("tempdir");
        let dir = tmp.path().join("albatros");
        let theme = albatros();
        theme.install(&dir).expect("install");

        age(&dir, rel, OLD, Some("0.0.9"));

        assert!(theme.install(&dir).expect("second add").is_empty());
        let lock = Lock::read(&dir).expect("lock");
        assert_eq!(
            lock.baudelaire, "0.0.9",
            "a run that wrote nothing restamped"
        );
        assert!(
            lock.state(&dir)
                .iter()
                .any(|file| file.rel == *rel && file.state == State::Pristine),
            "an untouched older file read as the author's"
        );
        theme.update(&dir, false).expect("update");
        assert_ne!(std::fs::read(dir.join(rel)).expect("read"), OLD);
    }

    #[test]
    fn an_update_rewrites_a_file_the_record_still_claims() {
        const OLD: &[u8] = b"// an older page.typ\n";
        let tmp = tempfile::tempdir().expect("tempdir");
        let dir = tmp.path().join("albatros");
        let theme = albatros();
        theme.install(&dir).expect("install");

        let rel = "templates/page.typ";
        age(&dir, rel, OLD, None);
        theme.update(&dir, false).expect("update");
        assert_ne!(std::fs::read(dir.join(rel)).expect("read"), OLD);
    }

    #[test]
    fn a_record_without_an_origin_is_a_shipped_theme() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let dir = tmp.path().join("albatros");
        albatros().install(&dir).expect("install");

        let mut lock = Lock::read(&dir).expect("lock");
        lock.origin = None;
        write(&dir, &lock);

        let origin = Lock::read(&dir).expect("lock").origin();
        assert_eq!(
            origin,
            crate::theme::Origin::Bundled {
                name: "albatros".to_owned()
            }
        );
    }

    /// What an earlier baudelaire's install leaves behind.
    fn age(dir: &std::path::Path, rel: &str, contents: &[u8], version: Option<&str>) {
        std::fs::write(dir.join(rel), contents).expect("age");
        let mut lock = Lock::read(dir).expect("lock");
        lock.files
            .insert(rel.to_owned(), crate::graph::Hash::of_bytes(contents).hex());
        if let Some(version) = version {
            lock.baudelaire = version.to_owned();
        }
        write(dir, &lock);
    }

    fn write(dir: &std::path::Path, lock: &Lock) {
        std::fs::write(
            dir.join(Lock::FILE),
            serde_json::to_vec(lock).expect("json"),
        )
        .expect("record");
    }
}
