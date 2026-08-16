//! Content hashing for cache invalidation.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// A blake3 content hash, compared to decide whether a cached artifact is
/// still valid.
///
/// Stored as the raw 32-byte digest; the hex form is materialized only where a
/// hash is used as a filename or written to the manifest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Hash([u8; 32]);

impl Hash {
    /// The hex digest, used as a content-addressed filename.
    pub fn hex(&self) -> String {
        blake3::Hash::from(self.0).to_hex().to_string()
    }

    /// The leading `len` hex digits, for the short forms spliced into names,
    /// saturating rather than slicing.
    pub fn short(&self, len: usize) -> String {
        let mut hex = self.hex();
        hex.truncate(len);
        hex
    }

    /// Hex digits of a digest naming its shard directory.
    const SHARD: usize = 2;

    /// The directory name holding content-addressed blobs, under whichever
    /// store's own directory.
    pub const OBJECTS: &'static str = "objects";

    /// This digest's blob path under `dir`: `<dir>/objects/ab/abcdef..`.
    ///
    /// The one layout, shared by the page store ([`crate::graph::Objects`]) and
    /// the processed-asset memo (`engine::asset::memo`), which must agree or a
    /// `clean` walks one and not the other.
    pub fn object(&self, dir: &Path) -> PathBuf {
        let hex = self.hex();
        let (shard, _) = hex.split_at(Self::SHARD.min(hex.len()));
        dir.join(Self::OBJECTS).join(shard).join(&hex)
    }

    /// Hash a file's bytes, or `None` if it can't be read.
    pub fn of_file(path: &Path) -> Option<Self> {
        Some(Self::of_bytes(&std::fs::read(path).ok()?))
    }

    /// Hash arbitrary bytes.
    pub fn of_bytes(bytes: &[u8]) -> Self {
        Self(blake3::hash(bytes).into())
    }

    /// Fingerprint any [`std::hash::Hash`] value with blake3, used to hash
    /// structured data (e.g. the whole [`crate::config::Config`]) without
    /// serializing it to a string first.
    pub fn of<T: std::hash::Hash>(value: &T) -> Self {
        let mut hasher = Blake3Hasher(blake3::Hasher::new());
        value.hash(&mut hasher);
        Self(hasher.0.finalize().into())
    }
}

/// The emitted name of an asset: the authored path with a short content digest
/// spliced in before the extension (`css/app.css` -> `css/app.<digest>.css`).
///
/// One owner for the rule and for the digest's length, because the asset
/// pipeline and the render pass name the same kind of artifact and have to
/// agree byte for byte.
pub struct AssetName<'a> {
    path: &'a Path,
    /// Spliced in after the stem, the extension kept; `None` names the file as
    /// authored, which is not an empty suffix, since rebuilding the name at all
    /// would drop an extension the platform can spell but UTF-8 cannot.
    suffix: Option<String>,
}

impl<'a> AssetName<'a> {
    /// Hex digits of the digest spliced into a fingerprinted name.
    pub const LEN: usize = 16;

    pub fn new(path: &'a Path, suffix: Option<String>) -> Self {
        Self { path, suffix }
    }

    /// The suffix that names a file by its content: a `.` and the leading
    /// [`Self::LEN`] hex digits of `bytes`' digest.
    pub fn digest(bytes: &[u8]) -> String {
        format!(".{}", Hash::of_bytes(bytes).short(Self::LEN))
    }

    /// The name in place, parent directories kept: `css/app.<digest>.css` for
    /// `css/app.css`.
    pub fn path(&self) -> PathBuf {
        match &self.suffix {
            Some(suffix) => self.path.with_file_name(self.spliced(suffix)),
            None => self.path.to_path_buf(),
        }
    }

    /// The bare file name, parent directories dropped, as an externalized image
    /// is served flat out of the asset root.
    pub fn file(&self) -> String {
        match &self.suffix {
            Some(suffix) => self.spliced(suffix),
            None => self
                .path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned(),
        }
    }

    /// The file name rebuilt: `suffix` after the stem, the extension echoed as
    /// authored and never invented for a file that has none.
    fn spliced(&self, suffix: &str) -> String {
        let stem = self
            .path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or_default();
        match self.path.extension().and_then(|ext| ext.to_str()) {
            Some(ext) => format!("{stem}{suffix}.{ext}"),
            None => format!("{stem}{suffix}"),
        }
    }
}

/// The identity of the thing that produced a cached artifact: everything that
/// can change generated output with no source, config, or dependency changing.
///
/// Every persisted cache folds this into its fingerprint, or an upgrade that
/// renders different markup from the same inputs leaves every page a hit.
#[derive(Debug, Clone, PartialEq, Eq, std::hash::Hash)]
pub struct Renderer {
    baudelaire: &'static str,
    /// The embedded typst compiler's version, which owns HTML export.
    typst: &'static str,
    schema: u32,
}

impl Renderer {
    /// The cache layout this binary writes and trusts; bump it whenever a
    /// manifest or entry means something new without looking different, or the
    /// same inputs start rendering different markup, since a warm entry reads
    /// as valid under either change.
    const SCHEMA: u32 = 20;

    pub fn current() -> Self {
        Self {
            baudelaire: crate::VERSION,
            typst: typst::utils::version().raw(),
            schema: Self::SCHEMA,
        }
    }
}

/// Serialized as its hex string, so the on-disk manifest stays human-readable.
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
    /// projection.
    fn finish(&self) -> u64 {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::{AssetName, Hash};
    use std::path::{Path, PathBuf};

    fn name<'a>(path: &'a str, suffix: &str) -> AssetName<'a> {
        AssetName::new(
            Path::new(path),
            (!suffix.is_empty()).then(|| suffix.to_owned()),
        )
    }

    #[test]
    fn a_path_keeps_its_directories() {
        assert_eq!(
            name("css/app.css", ".abc123").path(),
            PathBuf::from("css/app.abc123.css")
        );
        assert_eq!(
            name("img/photo.jpg", "-480").path(),
            PathBuf::from("img/photo-480.jpg")
        );
    }

    #[test]
    fn an_extensionless_name_never_gains_an_extension() {
        assert_eq!(
            name("LICENSE", ".abc123").path(),
            PathBuf::from("LICENSE.abc123")
        );
        assert_eq!(name("dir/photo", "").file(), "photo");
        assert_eq!(name("dir/photo", ".abc123").file(), "photo.abc123");
    }

    #[test]
    fn a_file_name_uses_the_base_name_and_keeps_the_extension() {
        assert_eq!(name("content/blog/photo.png", "").file(), "photo.png");
        assert_eq!(name("photo.png", "").file(), "photo.png");
        assert_eq!(name("a/b/c.jpeg", "").file(), "c.jpeg");
        assert_eq!(name("dir/photo.png", ".abc123").file(), "photo.abc123.png");
    }

    #[test]
    fn a_name_preserves_extension_case_and_compound_names() {
        assert_eq!(name("dir/Photo.PNG", "").file(), "Photo.PNG");
        assert_eq!(name("archive.tar.gz", "").file(), "archive.tar.gz");
        assert_eq!(
            name("archive.tar.gz", ".abc123").file(),
            "archive.tar.abc123.gz"
        );
    }

    #[test]
    fn an_unsuffixed_path_is_left_as_authored() {
        assert_eq!(name("css/app.css", "").path(), PathBuf::from("css/app.css"));
    }

    #[test]
    fn a_digest_is_derived_from_the_bytes_at_the_shared_length() {
        let digest = AssetName::digest(b"body{}");
        assert_eq!(digest, format!(".{}", Hash::of_bytes(b"body{}").short(16)));
        assert_eq!(digest.len(), AssetName::LEN + 1);
        assert_ne!(digest, AssetName::digest(b"body{ }"));
        assert_eq!(
            name("app.css", &digest).file(),
            name("dir/app.css", &digest).file()
        );
    }
}
