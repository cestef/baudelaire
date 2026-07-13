//! Per-build memo of file content hashes.
//!
//! Cache validation hashes every dependency of every page. Shared inputs — a
//! layout template, a theme module imported by hundreds of pages — would
//! otherwise be read and hashed once *per dependent page*. A file's content is
//! fixed for the duration of a build, so hashing it once and reusing the digest
//! is exact; this memo turns that per-page work into per-file work.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use parking_lot::Mutex;

use crate::graph::Hash;

/// A concurrent path → digest cache, valid for one build. Misses (an unreadable
/// file) are memoized as `None` so a missing dependency isn't re-stat'd per page.
#[derive(Default)]
pub struct FileDigests {
    map: Mutex<HashMap<PathBuf, Option<Hash>>>,
}

impl FileDigests {
    /// The content hash of `path`, computed once and reused. Concurrent first
    /// callers for the same path may both hash it — harmless, as the content is
    /// identical — but every later caller hits the memo.
    pub fn of(&self, path: &Path) -> Option<Hash> {
        if let Some(hash) = self.map.lock().get(path) {
            return *hash;
        }
        let hash = Hash::of_file(path);
        self.map.lock().insert(path.to_owned(), hash);
        hash
    }
}
