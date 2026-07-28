//! Resolves the `baudelaire:asset:` image markers left by
//! [`crate::world::image_rule::IMAGE_RULE`].
//!
//! The image show rule replaces typst's inline base64 with a marker carrying the
//! source file's project-relative path. This transform rewrites each marked
//! `<img src>` to the URL the file is served at (`/assets/<name>`) and records
//! the `(name, source)` pair so the engine can copy the file into `dist`. Naming
//! follows the asset pipeline: fingerprinted (`photo.<hash>.png`) when
//! `output { assets { fingerprint } }` is on, else the plain filename.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use typst_html::{HtmlDocument, attr, tag};

use crate::config::Config;
use crate::engine::asset::FINGERPRINT_LEN;
use crate::graph::Hash;

use super::{Cx, ElementExt, Transform};
use crate::world::image_rule::MARKER;

/// A typst-embedded image lifted out to a file: the filename it is served under
/// (relative to the asset directory) and the source file to copy from. Recorded
/// per page so a cache hit can re-copy the file without recompiling.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageRef {
    pub name: String,
    pub source: PathBuf,
}

/// The [`Transform`] that turns image markers into served asset references.
pub(super) struct Externalize;

impl Transform for Externalize {
    fn enabled(&self, config: &Config) -> bool {
        config.images.externalize(&config.html)
    }

    fn apply(&self, doc: &mut HtmlDocument, cx: &mut Cx<'_>) {
        let config = cx.config;
        let root = cx.root;
        // Gather markers while walking, then record: the walk borrows the DOM
        // mutably, so the `cx` accumulator is written after it finishes.
        let mut refs = Vec::new();
        doc.root_mut().walk(&mut |element| {
            if element.tag != tag::img {
                return;
            }
            element.rewrite(&[attr::src], |src| {
                let vpath = src.strip_prefix(MARKER)?;
                let (name, source) = resolve(vpath, root, config);
                refs.push(ImageRef {
                    name: name.clone(),
                    source,
                });
                Some(format!("/{}/{name}", config.asset_name()))
            });
        });
        cx.found.images.extend(refs);
    }
}

/// The served filename and source path for a marker's virtual path. The name is
/// fingerprinted (content hash spliced in) when asset fingerprinting is on, so
/// externalized images cache far-future like every other asset; otherwise the
/// plain filename is kept. A hash read that fails falls back to the plain name;
/// the engine's copy then surfaces the unreadable source.
fn resolve(vpath: &str, root: &Path, config: &Config) -> (String, PathBuf) {
    let source = root.join(vpath);
    let digest = config
        .asset
        .fingerprint
        .then(|| crate::fs::read(&source).ok())
        .flatten()
        .map(|bytes| Hash::of_bytes(&bytes).short(FINGERPRINT_LEN));
    (name(vpath, digest.as_deref()), source)
}

/// The served filename for a source path: its base name, with `digest` spliced
/// before the extension when fingerprinting. Subdirectories are dropped (only
/// the file name is served) and the extension is kept as authored.
fn name(vpath: &str, digest: Option<&str>) -> String {
    let path = Path::new(vpath);
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or(vpath);
    let ext = path.extension().and_then(|e| e.to_str());
    match (digest, ext) {
        (Some(digest), Some(ext)) => format!("{stem}.{digest}.{ext}"),
        (Some(digest), None) => format!("{stem}.{digest}"),
        (None, Some(ext)) => format!("{stem}.{ext}"),
        (None, None) => stem.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::name;

    #[test]
    fn name_uses_the_base_name_and_keeps_the_extension() {
        assert_eq!(name("content/blog/photo.png", None), "photo.png");
        assert_eq!(name("photo.png", None), "photo.png");
        assert_eq!(name("a/b/c.jpeg", None), "c.jpeg");
    }

    #[test]
    fn name_splices_the_digest_before_the_extension() {
        assert_eq!(name("dir/photo.png", Some("abc123")), "photo.abc123.png");
    }

    #[test]
    fn name_handles_an_extensionless_source() {
        assert_eq!(name("dir/photo", None), "photo");
        assert_eq!(name("dir/photo", Some("abc123")), "photo.abc123");
    }

    #[test]
    fn name_preserves_extension_case_and_compound_names() {
        // The extension is echoed verbatim (matched case-insensitively elsewhere,
        // but never rewritten), and only the final component is dropped.
        assert_eq!(name("dir/Photo.PNG", None), "Photo.PNG");
        assert_eq!(name("archive.tar.gz", None), "archive.tar.gz");
    }
}
