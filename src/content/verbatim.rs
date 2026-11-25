//! What a page ships beside itself untouched: its `verbatim` frontmatter,
//! resolved against the page's own directory and the directory it is served
//! from.

use std::path::{Path, PathBuf};

use crate::content::{Collection, Page};
use crate::error::{ContentError, Result};
use crate::fs::Contained;

/// One `verbatim` entry of one page: the file or folder it names, and where its
/// copy lands under `dist`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Verbatim {
    pub source: PathBuf,
    pub output: PathBuf,
}

impl Verbatim {
    /// What `entry` names beside the page at `page`, or `None` when it names
    /// nothing there; a symlink is not followed out of the page's directory.
    pub fn locate(page: &Path, entry: &str) -> Option<PathBuf> {
        let path = Contained::new(entry)?.under_real(page.parent()?)?;
        path.exists().then_some(path)
    }

    /// Every entry `page` declared, in the order it declared them.
    ///
    /// Frontmatter is checked when it is read, so this fails only for a file
    /// removed since a cached read of it.
    pub fn of(page: &Page) -> Result<Vec<Self>> {
        let served = page.output.parent().unwrap_or(&page.output);
        page.frontmatter
            .verbatim
            .iter()
            .map(|entry| {
                let source = Self::locate(&page.source, entry)
                    .ok_or_else(|| ContentError::verbatim_missing(&page.source, "", None, entry))?;
                Ok(Self {
                    source,
                    output: served.join(entry),
                })
            })
            .collect()
    }

    /// Drop every page whose source sits inside another page's entry: a
    /// verbatim folder is shipped as it is, so a `.md` in it is a file and not a
    /// page.
    pub fn withhold(collections: &mut [Collection]) -> Result<()> {
        let mut shipped = Vec::new();
        for page in collections.iter().flat_map(|c| &c.pages) {
            shipped.extend(Self::of(page)?);
        }
        if shipped.is_empty() {
            return Ok(());
        }
        for collection in collections {
            collection.pages.retain(|page| {
                !shipped
                    .iter()
                    .any(|entry| page.source.starts_with(&entry.source))
            });
        }
        Ok(())
    }
}
