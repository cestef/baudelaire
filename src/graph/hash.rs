//! Content hashing for cache invalidation.

use std::path::Path;

use serde::{Deserialize, Serialize};

/// A blake3 content hash, compared to decide whether a cached artifact is
/// still valid.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Hash(String);

impl Hash {
    /// The hex digest as a string slice — used as a content-addressed filename.
    pub fn hex(&self) -> &str {
        &self.0
    }

    /// Hash a file's bytes, or `None` if it can't be read.
    pub fn of_file(path: &Path) -> Option<Self> {
        Some(Self::of_bytes(&std::fs::read(path).ok()?))
    }

    /// Hash arbitrary bytes.
    pub fn of_bytes(bytes: &[u8]) -> Self {
        Self(blake3::hash(bytes).to_hex().to_string())
    }

    /// Fingerprint any [`std::hash::Hash`] value with blake3 — used to hash
    /// structured data (e.g. the whole [`crate::config::Config`]) without
    /// serializing it to a string first.
    pub fn of<T: std::hash::Hash>(value: &T) -> Self {
        let mut hasher = Blake3Hasher(blake3::Hasher::new());
        value.hash(&mut hasher);
        Self(hasher.0.finalize().to_hex().to_string())
    }

    /// A content fingerprint of every file under `dir`, sorted by relative path
    /// (an empty directory, or an absent one, hashes to a stable empty value).
    /// Used to invalidate pages that inline asset bytes (`embed`) — a change the
    /// per-file dependency tracker cannot otherwise see, since typst never reads
    /// the embedded files.
    pub fn of_dir(dir: &Path) -> Self {
        let mut files: Vec<(String, Hash)> = Vec::new();
        Self::collect(dir, dir, &mut files);
        files.sort_by(|(a, _), (b, _)| a.cmp(b));
        Self::of(&files)
    }

    fn collect(root: &Path, dir: &Path, out: &mut Vec<(String, Hash)>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                Self::collect(root, &path, out);
            } else if let Some(hash) = Self::of_file(&path) {
                let rel = path
                    .strip_prefix(root)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .into_owned();
                out.push((rel, hash));
            }
        }
    }
}

/// Adapts blake3 to [`std::hash::Hasher`] so any `Hash` value's bytes stream
/// straight into a strong content hash.
struct Blake3Hasher(blake3::Hasher);

impl std::hash::Hasher for Blake3Hasher {
    fn write(&mut self, bytes: &[u8]) {
        self.0.update(bytes);
    }

    /// Unused: the full digest is read via `finalize`, not this 64-bit
    /// projection. Required by the trait.
    fn finish(&self) -> u64 {
        0
    }
}
