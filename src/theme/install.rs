//! Vendoring a fetched theme: writing it into the project, recording which
//! bytes were ours, and telling the two apart ever after.
//!
//! None of this knows where the files came from. A theme is *vendored*: once
//! the copy is in the project it is committed with it and yours to edit, so the
//! only thing worth recording is which bytes baudelaire wrote, and that reads
//! the same whether they came out of the binary, a repository, or a directory
//! next door.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::source::{Fetched, Origin};
use crate::error::{Result, ThemeError};
use crate::graph::Hash;

/// What one file of an installed theme is, compared against the copy that was
/// written: the vocabulary `update` and `remove` act on, and `list` reports.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    /// Byte-identical to what was installed: safe to replace or delete.
    Pristine,
    /// Present and changed since it was installed: the author's, not ours.
    Edited,
    /// Recorded as installed and no longer on disk: deleted deliberately, so
    /// neither `update` nor `remove` puts it back.
    Gone,
    /// In the fetched theme and absent from this copy: a file this version adds.
    Added,
    /// In the fetched theme, here, and never ours: an install found it and
    /// skipped it. The author's whatever it holds, so `update` replaces it only
    /// under `force` and `remove` never deletes it: taking our copy off is no
    /// licence to delete a file we did not write.
    Yours,
}

/// One file of an installed theme, with what became of it.
pub struct Tracked {
    /// Path relative to the theme directory.
    pub rel: PathBuf,
    pub state: State,
}

/// The record an install leaves inside a theme directory: which theme it is,
/// where it came from, which baudelaire wrote it, and what each of its files
/// digested to then.
///
/// This is the whole of baudelaire's package state, and it is deliberately
/// small. There is no version to resolve later, because nothing is resolved
/// later: the files are in the project. What the record buys is the answer to
/// one question, which of these files are still ours, and that is what lets
/// `update` replace the ones you have not touched and `remove` refuse to delete
/// work.
///
/// It travels inside the theme directory, beside the files it describes, the
/// way a vendored crate carries its own checksums.
#[derive(Debug, Serialize, Deserialize)]
pub struct Lock {
    /// The name this copy is known by.
    pub theme: String,
    /// Where it came from, so `update` can go back for it without being told
    /// again.
    ///
    /// Absent in a record written before a theme could come from anywhere but
    /// the binary, which is exactly what those copies were: see
    /// [`Lock::origin`].
    origin: Option<Origin>,
    /// The baudelaire that wrote it, so `update` can say what it is updating
    /// from.
    pub baudelaire: String,
    /// Relative path to the digest baudelaire's own copy of the file has.
    files: BTreeMap<String, String>,
}

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

    /// Where this copy came from. A record with none was written when the only
    /// source was the binary, and named the shipped theme in `theme`, so that
    /// is what it means.
    pub fn origin(&self) -> Origin {
        self.origin.clone().unwrap_or_else(|| Origin::Bundled {
            name: self.theme.clone(),
        })
    }

    /// What each file the record claims is now, judged against the disk alone.
    ///
    /// Deliberately without the source's own file list: `list` and `info` report
    /// on a copy, and a report that had to fetch the theme first would put a
    /// clone or a download behind reading what is already in the project.
    /// [`Fetched::state`] is the fuller view, for the two verbs that have the
    /// files in hand anyway.
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

    /// Whether the record claims `rel`.
    fn claims(&self, rel: &Path) -> bool {
        self.files.contains_key(&rel.to_string_lossy().into_owned())
    }

    /// Take an installed copy back off, and report what each of its files was.
    ///
    /// Files still ours go; anything you edited stays unless `force`, and so
    /// does anything that was never ours, at any force: taking our copy off is
    /// no licence to delete a file we did not write. The record goes only when
    /// nothing it tracks is left, so a `remove` that kept your edits can still
    /// be finished with `--force` later. The directory itself goes once it is
    /// empty.
    ///
    /// Nothing is fetched: what a copy is, is what its own record says it is,
    /// so removing a theme never reaches for the network.
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
            match remove {
                true => crate::fs::remove_file(dir.join(&file.rel))?,
                false => kept |= file.state == State::Edited,
            }
        }
        if !kept {
            crate::fs::remove_file(dir.join(Self::FILE))?;
            Self::prune(dir);
        }
        Ok(tracked)
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

    /// The files an installed copy holds, whatever its record claims: what a
    /// report on a theme this binary cannot fetch has to read from instead.
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
    /// Write the theme's files under `dir`, skipping any that are already
    /// there, and report what was written. An existing file is the author's: a
    /// second install over an edited copy must not silently undo it.
    ///
    /// The copy is recorded in a [`Lock`] beside the files, which is what later
    /// tells your edits from ours.
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

    /// Bring an installed copy up to this fetch, file by file.
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
        let tracked = self.state(dir, Lock::read(dir).as_ref());
        let mut written = Vec::new();
        for file in &tracked {
            let Some(contents) = self.files.get(&file.rel) else {
                continue;
            };
            let replace = match file.state {
                State::Pristine | State::Added => true,
                State::Edited | State::Yours => force,
                // Deleted on purpose: an update that puts a file back is an
                // update that undoes a decision.
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

    /// What each of this copy's files is, the theme's own list included: a file
    /// the record does not claim is one this version adds, or one that was
    /// already there when the install ran and was skipped. On disk tells the two
    /// apart, and they are not the same file at all: the first is ours to write,
    /// the second is the author's to keep.
    ///
    /// A copy with no record is entirely the author's, so every file of it reads
    /// as edited.
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
    /// left, digesting the bytes that were fetched.
    ///
    /// Never what is on disk. Digesting the disk would record a file kept
    /// *because* it was edited as though baudelaire had written it, and the
    /// next `update` would overwrite the author's work as `Pristine`. Digesting
    /// what was fetched says only what this copy of the theme is, which is the
    /// one claim the record is entitled to make: a file that differs from it is
    /// the author's, whether they changed it after an install or had it before
    /// one.
    ///
    /// And only what was *written*. A file already there was skipped, so this
    /// run learned nothing about it: an entry an earlier install left stands, at
    /// the digest that install wrote, and a file that was never ours gains no
    /// entry at all. Re-recording the whole set was how a second `add` broke
    /// `update`: every file an older baudelaire had written came back with
    /// *this* binary's digest, so an untouched file read as `Edited` and update
    /// kept it, release after release.
    fn claim(&self, dir: &Path, written: &[PathBuf]) -> Result<()> {
        let previous = Lock::read(dir).filter(|lock| lock.theme == self.name);
        // Nothing of ours here and nothing written: a directory that is entirely
        // the author's stays that way rather than gaining a record of no files.
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
            // What wrote the files that are here. A run that wrote none leaves
            // the record describing the baudelaire that did.
            baudelaire: match written.is_empty() {
                true => previous.map_or_else(|| crate::VERSION.to_owned(), |lock| lock.baudelaire),
                false => crate::VERSION.to_owned(),
            },
            files,
        };
        let json = serde_json::to_vec_pretty(&lock)
            .map_err(|why| ThemeError::lock(dir.join(Lock::FILE).display(), why))?;
        crate::fs::write(dir.join(Lock::FILE), json)
    }

    /// Write one file, making the directories above it first.
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

    /// A shipped theme, as any source hands one over: the fixture every case
    /// here vendors, because what is being tested is the vendoring and not the
    /// fetching.
    fn albatros() -> crate::theme::Fetched {
        Bundled::find("albatros").expect("shipped").fetched()
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

    /// The bug a second `add` used to leave behind: it writes no file, so it
    /// must record no file either. Claiming the whole fetched set re-digested
    /// every file an older baudelaire had written, turning an untouched one into
    /// an `Edited` one that `update` then kept for good.
    #[test]
    fn a_stray_add_does_not_disown_an_older_install() {
        const OLD: &[u8] = b"/* the 0.0.9 stylesheet */\n";
        let rel = "assets/style.css";
        let tmp = tempfile::tempdir().expect("tempdir");
        let dir = tmp.path().join("albatros");
        let theme = albatros();
        theme.install(&dir).expect("install");

        // An older baudelaire's copy: an older file, and a record that says so.
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

    /// The other half: what the record *does* claim is replaced. An older
    /// baudelaire left an older file and a record of it, and that is exactly
    /// the file an update exists to bring forward.
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

    /// A copy from before a theme could come from anywhere but the binary: its
    /// record names no source, and that is not a copy of unknown provenance but
    /// a shipped one, so it still updates from the shelf.
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

    /// Put an older file at `rel` and a record that claims it, which is what an
    /// earlier baudelaire's install leaves behind.
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
