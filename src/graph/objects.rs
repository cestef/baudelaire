//! The build cache's content-addressed object store.
//!
//! Rendered HTML lives here, one file per distinct markup, named by its own
//! blake3 digest and sharded by the first two hex digits so no directory grows
//! unbounded. The manifest holds only the digest, so a load parses metadata
//! instead of every page's markup, identical output is stored once, and an
//! unchanged blob is never rewritten.

use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use rayon::prelude::*;

use crate::error::Result;
use crate::graph::Hash;

/// Subdirectory holding content-addressed HTML blobs.
const OBJECTS: &str = "objects";

/// Digits of a blob's hex digest used as its shard directory.
const SHARD: usize = 2;

/// The blob store under one cache directory.
pub(super) struct Objects {
    dir: PathBuf,
    /// Blobs found on disk not matching their own content address, to be
    /// rewritten by [`Objects::write`] rather than left broken forever.
    corrupt: HashSet<Hash>,
}

impl Objects {
    /// The store under a cache directory. Nothing is touched until a read or a
    /// write asks for it.
    pub(super) fn new(cache: &Path) -> Self {
        Self {
            dir: cache.join(OBJECTS),
            corrupt: HashSet::new(),
        }
    }

    /// The blob's contents, or `None` when it is absent or does not match the
    /// address that names it.
    ///
    /// A torn write leaves bytes whose name claims a digest they do not have.
    /// The mismatch is a miss, and the bad file is remembered so the next
    /// [`Objects::write`] overwrites it: it is still referenced (so the prune
    /// keeps it) and its path already exists (so a write would be skipped),
    /// which left that page missing on every build from then on with no way to
    /// self-heal.
    pub(super) fn read(&mut self, blob: &Hash) -> Option<String> {
        let html = fs::read_to_string(self.path(blob)).ok()?;
        if Hash::of_bytes(html.as_bytes()) != *blob {
            self.corrupt.insert(*blob);
            return None;
        }
        Some(html)
    }

    /// Write every blob that is not already stored, in parallel: the files are
    /// independent and content-addressed, so an unchanged page's markup is never
    /// rewritten. Two pages sharing markup stage one write (keyed by path), so a
    /// duplicate can never race itself.
    pub(super) fn write<'a>(
        &self,
        blobs: impl IntoIterator<Item = (&'a Hash, &'a str)>,
    ) -> Result<()> {
        let pending: BTreeMap<PathBuf, &str> = blobs
            .into_iter()
            .filter(|(blob, _)| self.stale(blob))
            .map(|(blob, contents)| (self.path(blob), contents))
            .collect();
        pending.par_iter().try_for_each(|(path, contents)| {
            if let Some(parent) = path.parent() {
                crate::fs::create_dir_all(parent)?;
            }
            Self::atomic(path, contents.as_bytes())
        })
    }

    /// Remove object files not in `live`. Best-effort: the cache is
    /// regenerable, so a housekeeping failure never fails a build.
    pub(super) fn prune(&self, live: &HashSet<Hash>) {
        let live: HashSet<String> = live.iter().map(Hash::hex).collect();
        let Ok(shards) = fs::read_dir(&self.dir) else {
            return;
        };
        for shard in shards.flatten() {
            let Ok(blobs) = fs::read_dir(shard.path()) else {
                continue;
            };
            for blob in blobs.flatten() {
                let referenced = blob
                    .file_name()
                    .to_str()
                    .is_some_and(|name| live.contains(name));
                if !referenced {
                    let _ = fs::remove_file(blob.path());
                }
            }
        }
    }

    /// Write `bytes` to `path` via a temporary sibling and a rename, so a reader
    /// only ever sees the complete file (rename is atomic on the same
    /// filesystem). Shared with the manifest write, which needs the same
    /// guarantee. Content-addressed blobs are written once per build, so the
    /// fixed `.tmp` suffix never races itself.
    pub(super) fn atomic(path: &Path, bytes: &[u8]) -> Result<()> {
        let tmp = path.with_extension("tmp");
        crate::fs::write(&tmp, bytes)?;
        crate::fs::rename(&tmp, path)
    }

    /// Whether a blob still has to be written: it is missing, or the copy on
    /// disk was found corrupt this build.
    fn stale(&self, blob: &Hash) -> bool {
        self.corrupt.contains(blob) || !self.path(blob).exists()
    }

    /// Absolute path of a blob, sharded by hash prefix to keep any one
    /// directory small.
    fn path(&self, blob: &Hash) -> PathBuf {
        let hex = blob.hex();
        let (shard, _) = hex.split_at(SHARD.min(hex.len()));
        self.dir.join(shard).join(hex)
    }
}
