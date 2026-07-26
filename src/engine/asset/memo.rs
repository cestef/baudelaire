//! Cross-build memo for processed asset bytes.
//!
//! Without it the pipeline is fully non-incremental: every build re-runs oxipng
//! and a Lanczos3 downscale over every image, even one where nothing changed,
//! while page compilation right next to it is both cached and parallel.
//!
//! Only handlers whose output is a pure function of their own bytes and the
//! config are memoized (see [`super::Handler::pure`]). A stylesheet rewrites
//! references to *other* assets' hashed names and a script bundles a whole
//! import graph, so neither is keyed by its own bytes.
//!
//! Layout under the cache directory:
//!
//! ```text
//! assets/<key>.json          # what one source rendered to
//! assets/objects/ab/abcd..   # the bytes, content-addressed and shared
//! ```

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::config::Config;
use crate::graph::{Hash, Renderer};

use super::Variant;

/// What one source file rendered to: its primary output and any variants.
#[derive(Debug, Serialize, Deserialize)]
pub(super) struct Rendered {
    /// The handler's primary output, absent for a file it emits nothing for.
    primary: Option<Hash>,
    variants: Vec<Derived>,
}

/// One responsive candidate, its bytes stored under `blob` (absent when the
/// variant reuses the primary output's bytes).
#[derive(Debug, Serialize, Deserialize)]
struct Derived {
    rel: PathBuf,
    width: u32,
    blob: Option<Hash>,
}

/// The processed-asset memo.
pub(super) struct Memo {
    dir: PathBuf,
    /// Everything besides the source bytes that changes an asset's output.
    salt: Hash,
    enabled: bool,
}

impl Memo {
    pub(super) fn new(config: &Config) -> Self {
        Self {
            dir: config.cache.dir.join("assets"),
            salt: Hash::of(&(
                &config.asset,
                &config.images,
                config.asset_name(),
                Renderer::current(),
            )),
            enabled: config.cache.incremental,
        }
    }

    /// The key for `source`'s bytes at `rel`. `rel` is part of it because a
    /// handler may name its output after the path, not just the content.
    pub(super) fn key(&self, bytes: &[u8], rel: &Path) -> Hash {
        Hash::of(&(Hash::of_bytes(bytes), rel, self.salt))
    }

    /// The stored outputs for `key`, when every blob it names is still present.
    pub(super) fn get(&self, key: &Hash) -> Option<(Option<Vec<u8>>, Vec<Variant>)> {
        if !self.enabled {
            return None;
        }
        let json = std::fs::read(self.entry(key)).ok()?;
        let rendered: Rendered = serde_json::from_slice(&json).ok()?;
        let primary = match rendered.primary {
            Some(blob) => Some(self.blob(&blob)?),
            None => None,
        };
        let variants = rendered
            .variants
            .into_iter()
            .map(|derived| {
                let bytes = match derived.blob {
                    Some(blob) => Some(self.blob(&blob)?),
                    None => None,
                };
                Some(Variant {
                    rel: derived.rel,
                    width: derived.width,
                    bytes,
                })
            })
            .collect::<Option<Vec<_>>>()?;
        Some((primary, variants))
    }

    /// Record what `key` rendered to. Best-effort: a cache that cannot be
    /// written costs the next build the same work, nothing worse.
    pub(super) fn put(&self, key: &Hash, primary: Option<&[u8]>, variants: &[Variant]) {
        if !self.enabled {
            return;
        }
        let store = |bytes: &[u8]| {
            let hash = Hash::of_bytes(bytes);
            let path = self.object(&hash);
            if !path.exists()
                && let Some(parent) = path.parent()
            {
                let _ = std::fs::create_dir_all(parent);
                let _ = std::fs::write(&path, bytes);
            }
            hash
        };
        let rendered = Rendered {
            primary: primary.map(&store),
            variants: variants
                .iter()
                .map(|variant| Derived {
                    rel: variant.rel.clone(),
                    width: variant.width,
                    blob: variant.bytes.as_deref().map(&store),
                })
                .collect(),
        };
        let Ok(json) = serde_json::to_vec(&rendered) else {
            return;
        };
        let _ = std::fs::create_dir_all(&self.dir);
        let _ = std::fs::write(self.entry(key), json);
    }

    fn entry(&self, key: &Hash) -> PathBuf {
        self.dir.join(format!("{}.json", key.hex()))
    }

    /// Content-addressed blob path, sharded by hash prefix like the page store.
    fn object(&self, blob: &Hash) -> PathBuf {
        let hex = blob.hex();
        let (shard, _) = hex.split_at(2.min(hex.len()));
        self.dir.join("objects").join(shard).join(hex)
    }

    /// A blob's bytes, verified against the name that claims their hash so a
    /// torn write is a miss rather than a corrupt asset.
    fn blob(&self, hash: &Hash) -> Option<Vec<u8>> {
        let bytes = std::fs::read(self.object(hash)).ok()?;
        (Hash::of_bytes(&bytes) == *hash).then_some(bytes)
    }
}
