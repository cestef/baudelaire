//! Per-build memo of file content hashes, so a dependency shared by hundreds of
//! pages is read and hashed once.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use parking_lot::Mutex;

use crate::graph::Hash;

/// A concurrent path-to-digest cache, valid for one build; an unreadable file
/// is memoized as `None` rather than re-stat'd per page.
#[derive(Default)]
pub struct FileDigests {
    map: Mutex<HashMap<PathBuf, Option<Hash>>>,
}

impl FileDigests {
    /// The content hash of `path`, computed once and reused.
    pub fn of(&self, path: &Path) -> Option<Hash> {
        if let Some(hash) = self.map.lock().get(path) {
            return *hash;
        }
        let hash = Hash::of_file(path);
        self.map.lock().insert(path.to_owned(), hash);
        hash
    }
}
