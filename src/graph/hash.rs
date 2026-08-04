//! Content hashing for cache invalidation.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// A blake3 content hash, compared to decide whether a cached artifact is
/// still valid.
///
/// Stored as the raw 32-byte digest, not its hex string: comparison (the hot
/// path, every cache probe) is a fixed 32-byte memcmp with no allocation, and
/// the 64-char hex form is materialized only when a hash is used as a filename
/// or written to the manifest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Hash([u8; 32]);

impl Hash {
    /// The hex digest, materialized on demand, used as a content-addressed
    /// filename. Allocates; the raw bytes drive equality, so hot-path compares
    /// never call this.
    pub fn hex(&self) -> String {
        blake3::Hash::from(self.0).to_hex().to_string()
    }

    /// The leading `len` hex digits, for the short forms spliced into names
    /// (an asset fingerprint, a scope id). Saturates rather than slicing, so a
    /// caller can never index past the digest.
    pub fn short(&self, len: usize) -> String {
        let mut hex = self.hex();
        hex.truncate(len);
        hex
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
/// One owner for the rule and for the digest's length, because two layers name
/// the same kind of artifact and have to agree byte for byte: the asset pipeline
/// emits `app.<digest>.css`, and the render pass lifts a typst-embedded image
/// out to a file that must be indistinguishable from one the pipeline wrote.
/// Living beside [`Hash`], below both, is also what keeps `render` from reaching
/// into `engine` for a `usize`.
///
/// Where the name lands is the caller's choice, not the rule's: [`Self::path`]
/// keeps the parent directories, [`Self::file`] drops them.
pub struct AssetName<'a> {
    path: &'a Path,
    /// Spliced in after the stem, the extension kept. `None` means "name it as
    /// authored", which is not the same as an empty suffix: rebuilding the name
    /// at all would drop an extension the platform can spell but UTF-8 cannot.
    suffix: Option<String>,
}

impl<'a> AssetName<'a> {
    /// Hex digits of the digest spliced into a fingerprinted name. 16 hex chars
    /// = 64 bits of blake3: collision-free in practice for a site's asset set.
    pub const LEN: usize = 16;

    /// A name for `path` carrying `suffix`, or the authored name when there is
    /// none (`assets { fingerprint #false }`).
    pub fn new(path: &'a Path, suffix: Option<String>) -> Self {
        Self { path, suffix }
    }

    /// The suffix that names a file by its content: a `.` and the leading
    /// [`Self::LEN`] hex digits of `bytes`' digest. The other suffix in play is
    /// a responsive width (`-480`), which is the same splice with a suffix that
    /// is not a digest.
    pub fn digest(bytes: &[u8]) -> String {
        format!(".{}", Hash::of_bytes(bytes).short(Self::LEN))
    }

    /// The name in place, parent directories kept: the asset pipeline writes
    /// `css/app.<digest>.css` where it read `css/app.css`.
    pub fn path(&self) -> PathBuf {
        match &self.suffix {
            Some(suffix) => self.path.with_file_name(self.spliced(suffix)),
            None => self.path.to_path_buf(),
        }
    }

    /// The bare file name, parent directories dropped: an externalized image is
    /// served flat out of the asset root, whatever subdirectory of the project
    /// it was authored in.
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
    /// authored, and never invented for a file that has none (`LICENSE` must not
    /// become `LICENSE.<digest>`).
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
/// Every persisted cache folds this into its fingerprint. Without it, upgrading
/// baudelaire (a fixed renderer, a new transform, a different HTML shape) or the
/// typst it embeds leaves every page a cache *hit*, serving markup from the
/// previous renderer forever, and a change to the manifest's own layout reads
/// the old file as if it meant the same thing.
#[derive(Debug, Clone, PartialEq, Eq, std::hash::Hash)]
pub struct Renderer {
    /// This binary's version.
    baudelaire: &'static str,
    /// The embedded typst compiler's version, which owns HTML export.
    typst: &'static str,
    /// The on-disk cache layout. Bump by hand whenever a manifest or entry
    /// field changes meaning without changing shape, since serde would happily
    /// read the old file.
    schema: u32,
}

impl Renderer {
    /// 14: extraction and the asset pipeline agree about images. One that
    /// lives in the asset tree is referenced where the pipeline serves it
    /// rather than extracted a second time, and one lifted out of a page
    /// carries the responsive widths its `srcset` promised. An entry written
    /// before this records the duplicate extraction and no widths, so a cached
    /// page would keep copying source bytes over processed ones and would show
    /// a `srcset`-less image beside a freshly rendered one that has it.
    /// 13: a generated listing records its links too, so the orphan report can
    /// see that a page is reachable from an index. An entry written before this
    /// has none for a listing, so every page reachable only from one would be
    /// reported as linked from nowhere.
    /// 12: those recorded links keep their `#fragment`, so a backlink can say
    /// which section it aimed at. An entry written before this recorded the page
    /// alone, so a cached page's backlinks would be grouped under no section at
    /// all and a template grouping by one would show them nowhere.
    /// 11: `Outputs` carries the pages each page's own content links to, which
    /// the site-wide link graph is assembled from. An entry written before this
    /// records none, so a cached page would contribute no edges and the pages it
    /// links to would lose their backlinks out of a green build.
    /// 10: `Outputs` carries the digests of the page's inline scripts and
    /// styles. An entry written before this records none, so a cached page
    /// would contribute nothing to the generated policy and a browser would
    /// refuse to run the page's own inline script.
    /// 9: `Outputs` carries the page's lint findings and the ledger of what it
    /// ships. An entry written before this records neither, so a cached page
    /// would read as clean and weightless: the lint and budget gates would both
    /// pass on the second build of a site they failed on the first.
    /// 8: `Outputs` carries each page's heading ids and its `#fragment` links,
    /// for the site-wide deep-link check. An entry written before this records
    /// neither, so a cached page would look like it exposes no headings at all
    /// and every link into it would be reported as dangling.
    /// 7: embedded asset bytes became ordinary per-page dependencies, so the
    /// whole-tree `embeds` hash left the manifest fingerprint. An entry written
    /// before this lists no embedded file among its deps.
    /// 6: the responsive variant manifest and the processed-asset URL map both
    /// left the manifest fingerprint, for per-page `Entry::srcsets` and
    /// `Entry::assets`. An entry written before this records neither, so under
    /// the new narrower rules it would read as valid however those changed.
    /// 5: `Entry::deps` now covers what a page's social card compile read, and
    /// the card template's hash left the manifest fingerprint with it. An entry
    /// written before this records neither, so its card is validated against
    /// nothing at all and would be served stale for the life of the cache.
    /// 4: `Outputs` carries the page's head/body fragments for the single-file
    /// export. 3: `Entry` groups the render pass's results under `outputs`,
    /// which now also carries the page's broken links. 2: `Entry::deps` values
    /// became `Option<Hash>`, and manifest keys became project-relative.
    const SCHEMA: u32 = 14;

    pub fn current() -> Self {
        Self {
            baudelaire: crate::VERSION,
            typst: typst::utils::version().raw(),
            schema: Self::SCHEMA,
        }
    }
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

    /// The suffix lands between the stem and the extension, and the parent
    /// directories survive: this is the name the asset pipeline writes.
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

    /// An extensionless file must not grow one: `LICENSE` becomes
    /// `LICENSE.abc123`, never `LICENSE.abc123.<nothing>`.
    #[test]
    fn an_extensionless_name_never_gains_an_extension() {
        assert_eq!(
            name("LICENSE", ".abc123").path(),
            PathBuf::from("LICENSE.abc123")
        );
        assert_eq!(name("dir/photo", "").file(), "photo");
        assert_eq!(name("dir/photo", ".abc123").file(), "photo.abc123");
    }

    /// A file name drops the directories: an externalized image is served flat
    /// out of the asset root.
    #[test]
    fn a_file_name_uses_the_base_name_and_keeps_the_extension() {
        assert_eq!(name("content/blog/photo.png", "").file(), "photo.png");
        assert_eq!(name("photo.png", "").file(), "photo.png");
        assert_eq!(name("a/b/c.jpeg", "").file(), "c.jpeg");
        assert_eq!(name("dir/photo.png", ".abc123").file(), "photo.abc123.png");
    }

    #[test]
    fn a_name_preserves_extension_case_and_compound_names() {
        // The extension is echoed verbatim (matched case-insensitively
        // elsewhere, but never rewritten), and only the final component is
        // dropped.
        assert_eq!(name("dir/Photo.PNG", "").file(), "Photo.PNG");
        assert_eq!(name("archive.tar.gz", "").file(), "archive.tar.gz");
        assert_eq!(
            name("archive.tar.gz", ".abc123").file(),
            "archive.tar.abc123.gz"
        );
    }

    /// With no suffix the authored name is handed back untouched, rather than
    /// rebuilt from a stem and an extension.
    #[test]
    fn an_unsuffixed_path_is_left_as_authored() {
        assert_eq!(name("css/app.css", "").path(), PathBuf::from("css/app.css"));
    }

    /// The digest is the content's, at the one shared length, and equal bytes
    /// name equal files wherever they are read from.
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
