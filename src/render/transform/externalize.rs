//! Resolves the `baudelaire:asset:` image markers left by
//! [`crate::world::rules`], rewriting each marked `<img src>` to the URL the
//! file is served at and recording the source so the engine can copy it.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use typst_html::{HtmlDocument, attr, tag};

use crate::config::Config;
use crate::error::ImageError;
use crate::fs::Contained;
use crate::graph::AssetName;
use crate::render::Candidate;

use super::{Cx, DocumentExt, ElementExt, Transform};
use crate::world::rules::MARKER;

/// A typst-embedded image lifted out to a file, recorded per page so a cache
/// hit can re-copy it, and re-cut its variants, without recompiling.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageRef {
    pub name: String,
    pub source: PathBuf,
    /// Downscaled widths to write beside it, ascending, empty when the site
    /// asks for no variants or the source is too small to have any.
    #[serde(default)]
    pub widths: Vec<u32>,
}

/// Turns image markers into served asset references.
///
/// typst checks neither join a marker's path takes, so a hand-written marker
/// that escapes the root is refused rather than allowed around the compiler's
/// sandbox.
pub(super) struct Externalize;

impl Transform for Externalize {
    fn enabled(&self, config: &Config) -> bool {
        config.assets.images.externalize(&config.html)
    }

    fn apply(&self, doc: &mut HtmlDocument, cx: &mut Cx<'_>) {
        let config = cx.config;
        let root = cx.root;
        let mut refs = Vec::new();
        let mut failed = Vec::new();
        let mut variants = BTreeMap::new();
        doc.walk(|element| {
            if element.tag != tag::img {
                return;
            }
            element.rewrite(&[attr::src], |src| {
                let vpath = src.strip_prefix(MARKER)?;
                if Contained::new(vpath).is_none() {
                    failed.push(ImageError::escaping(vpath));
                    return None;
                }
                if let Some(url) = ImageRef::pipelined(vpath, root, config) {
                    return Some(url);
                }
                let image = ImageRef::of(vpath, root, config);
                let url = image.url(config);
                if !image.widths.is_empty() {
                    variants.insert(url.clone(), image.candidates(config));
                }
                refs.push(image);
                Some(url)
            });
        });
        cx.found.images.extend(refs);
        cx.found.invalid.extend(failed.into_iter().map(Into::into));
        cx.extracted.extend(variants);
    }
}

impl ImageRef {
    /// The URL an image *inside the asset tree* is already served at, or `None`
    /// for one anywhere else; such a file must be referenced where the pipeline
    /// put it rather than copied a second time under a name of its own.
    fn pipelined(vpath: &str, root: &Path, config: &Config) -> Option<String> {
        let rel = Path::new(vpath)
            .strip_prefix(Self::rooted(&config.paths.assets, root))
            .ok()?;
        Some(config.asset_url(rel))
    }

    /// A configured directory spelled the way a marker's `vpath` is: relative
    /// to the project root. A resolved config carries these absolute, so
    /// stripping the configured form as it stands matches nothing.
    fn rooted<'a>(dir: &'a Path, root: &Path) -> &'a Path {
        dir.strip_prefix(root).unwrap_or(dir)
    }

    /// The reference for a marker's virtual path, fingerprinted when asset
    /// fingerprinting is on. The name keeps the directories the image was
    /// authored under, so `posts/a/cover.png` and `posts/b/cover.png` are two
    /// files rather than one name claimed twice.
    fn of(vpath: &str, root: &Path, config: &Config) -> Self {
        let source = root.join(vpath);
        let digest = config
            .assets
            .fingerprint
            .then(|| crate::fs::read(&source).ok())
            .flatten()
            .map(|bytes| AssetName::digest(&bytes));
        let content = Self::rooted(&config.paths.content, root);
        let rel = Path::new(vpath)
            .strip_prefix(content)
            .unwrap_or_else(|_| Path::new(vpath));
        Self {
            name: AssetName::new(rel, digest)
                .path()
                .to_string_lossy()
                .into_owned(),
            widths: Self::widths(&source, config),
            source,
        }
    }

    /// The widths a `srcset` for this image can offer: the configured ones
    /// below the source's own, read from the file's header rather than by
    /// decoding it. Empty when the site asks for no variants or the file is not
    /// a raster this build can read.
    #[cfg(feature = "images")]
    fn widths(source: &Path, config: &Config) -> Vec<u32> {
        let responsive = &config.assets.images.responsive;
        if !responsive.enabled {
            return Vec::new();
        }
        let Ok((width, _)) = image::image_dimensions(source) else {
            return Vec::new();
        };
        responsive.applicable(width)
    }

    /// No encoder, no variants: a slim build copies rasters through, and a
    /// `srcset` naming files it will not cut is a page of dead candidates.
    #[cfg(not(feature = "images"))]
    fn widths(_source: &Path, _config: &Config) -> Vec<u32> {
        Vec::new()
    }

    fn url(&self, config: &Config) -> String {
        config.asset_url(Path::new(&self.name))
    }

    /// This image's `srcset` candidates: one per width, plus the source itself
    /// as the largest. Named through [`Self::variant`], the one rule the copy
    /// pass also names by, because the page is served before the bytes are cut.
    fn candidates(&self, config: &Config) -> Vec<Candidate> {
        self.widths
            .iter()
            .map(|&width| Candidate {
                url: config.asset_url(Path::new(&Self::variant(&self.name, width))),
                width,
            })
            .chain(self.source_candidate(config))
            .collect()
    }

    /// The source itself as the largest candidate, with the intrinsic width a
    /// browser needs to choose between it and the downscales.
    #[cfg(feature = "images")]
    fn source_candidate(&self, config: &Config) -> Option<Candidate> {
        let (width, _) = image::image_dimensions(&self.source).ok()?;
        Some(Candidate {
            url: self.url(config),
            width,
        })
    }

    /// With no encoder there are no variants to be the largest of.
    #[cfg(not(feature = "images"))]
    // The signature mirrors the `images`-on one.
    #[allow(clippy::unused_self)]
    fn source_candidate(&self, _config: &Config) -> Option<Candidate> {
        None
    }

    /// The served name of one width variant: `photo-480.png`, or
    /// `photo-480.<digest>.png` where the name it is cut from is fingerprinted.
    /// The extension is split off the file name alone, since a directory the
    /// image was authored under may itself hold a dot.
    pub fn variant(name: &str, width: u32) -> String {
        let (dir, file) = match name.rsplit_once('/') {
            Some((dir, file)) => (format!("{dir}/"), file),
            None => (String::new(), name),
        };
        let (stem, digest) = match file.split_once('.') {
            Some((stem, rest)) => (stem, format!(".{rest}")),
            None => (file, String::new()),
        };
        format!("{dir}{stem}-{width}{digest}")
    }
}

#[cfg(test)]
mod tests {
    use super::ImageRef;
    use crate::config::Config;
    use std::path::PathBuf;

    #[test]
    fn a_content_subdirectory_appears_once_in_every_candidate() {
        let config = Config::default();
        let image = ImageRef {
            name: "gallery/cover.png".to_owned(),
            source: PathBuf::from("content/gallery/cover.png"),
            widths: vec![30, 60],
        };

        assert_eq!(image.url(&config), "/assets/gallery/cover.png");
        let urls: Vec<String> = image
            .candidates(&config)
            .into_iter()
            .map(|candidate| candidate.url)
            .collect();
        assert_eq!(
            urls,
            [
                "/assets/gallery/cover-30.png",
                "/assets/gallery/cover-60.png"
            ]
        );
    }

    #[test]
    fn an_image_at_the_content_root_gains_no_directory() {
        let config = Config::default();
        let image = ImageRef {
            name: "pic.png".to_owned(),
            source: PathBuf::from("content/pic.png"),
            widths: vec![30],
        };

        assert_eq!(image.url(&config), "/assets/pic.png");
        assert_eq!(image.candidates(&config)[0].url, "/assets/pic-30.png");
    }
}
