//! Content hashing for cache invalidation.

use std::path::Path;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// A blake3 content hash, compared to decide whether a cached artifact is
/// still valid.
///
/// Stored as the raw 32-byte digest, not its hex string: comparison (the hot
/// path — every cache probe) is a fixed 32-byte memcmp with no allocation, and
/// the 64-char hex form is materialized only when a hash is used as a filename
/// or written to the manifest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Hash([u8; 32]);

impl Hash {
    /// The hex digest, materialized on demand — used as a content-addressed
    /// filename. Allocates; the raw bytes drive equality, so hot-path compares
    /// never call this.
    pub fn hex(&self) -> String {
        blake3::Hash::from(self.0).to_hex().to_string()
    }

    /// Hash a file's bytes, or `None` if it can't be read.
    pub fn of_file(path: &Path) -> Option<Self> {
        Some(Self::of_bytes(&std::fs::read(path).ok()?))
    }

    /// Hash arbitrary bytes.
    pub fn of_bytes(bytes: &[u8]) -> Self {
        Self(blake3::hash(bytes).into())
    }

    /// Fingerprint any [`std::hash::Hash`] value with blake3 — used to hash
    /// structured data (e.g. the whole [`crate::config::Config`]) without
    /// serializing it to a string first.
    pub fn of<T: std::hash::Hash>(value: &T) -> Self {
        let mut hasher = Blake3Hasher(blake3::Hasher::new());
        value.hash(&mut hasher);
        Self(hasher.0.finalize().into())
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

/// A value that can fingerprint itself into a [`Hash`], for cache invalidation.
///
/// One shared vocabulary for "the content digest of this thing" — a page-link
/// map, the processed-asset map, a publishable record. Every implementor folds
/// its digest into some cache and re-derives dependent work when it changes, so
/// they all speak in [`Hash`] and compose the same way. Implement it, don't call
/// [`Hash::of`] ad-hoc, when a type has a canonical fingerprint of its own.
pub trait Fingerprint {
    /// This value's content fingerprint. Stable across runs for equal content.
    fn fingerprint(&self) -> Hash;
}

/// Serialized as its hex string, so the on-disk manifest stays human-readable
/// (and unchanged from when the digest was stored as a `String`).
impl Serialize for Hash {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.hex())
    }
}

impl<'de> Deserialize<'de> for Hash {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let hex = <&str>::deserialize(deserializer)?;
        blake3::Hash::from_hex(hex)
            .map(|h| Self(h.into()))
            .map_err(serde::de::Error::custom)
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
