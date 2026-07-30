//! Where Typst packages come from.
//!
//! Two things resolve `@preview/name:version` during a build: the compiler, for
//! a page's own `#import`, and [`Theme`](crate::theme::Theme), for the templates
//! and assets a site borrows. Both go through the store built here, so a site
//! pointing at a mirror points *everything* at it: a theme served from the
//! official registry while its neighbouring imports come from a mirror is not a
//! configuration anyone asked for.
//!
//! A mirror only changes where the `preview` namespace is fetched from. Every
//! other namespace is served from the local package directories, which is what
//! makes an unpublished theme resolvable, and no namespace is ever fetched from
//! anywhere else.

use typst_kit::downloader::SystemDownloader;
use typst_kit::packages::{FsPackages, SystemPackages, UniversePackages};

/// The registry the `preview` namespace is downloaded from: a mirror when the
/// site names one, the official one otherwise.
pub struct Registry<'a>(pub Option<&'a str>);

impl From<Registry<'_>> for SystemPackages {
    fn from(Registry(url): Registry<'_>) -> Self {
        let downloader = SystemDownloader::new(super::USER_AGENT);
        match url {
            // The local data and cache directories stay exactly as the default
            // store has them: a mirror redirects downloads, it does not move
            // where a package lands or where a local one is looked up.
            Some(url) => Self::from_parts(
                FsPackages::system_data(),
                FsPackages::system_cache(),
                UniversePackages::with_url(downloader, url),
            ),
            None => Self::new(downloader),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A configured mirror is what the store downloads from, and no mirror is
    /// the official registry: the two ends of the only decision this type makes.
    #[test]
    fn a_mirror_replaces_the_registry_url() {
        let mirrored = SystemPackages::from(Registry(Some("https://packages.example.net")));
        assert_eq!(mirrored.universe().url(), "https://packages.example.net");

        let official = SystemPackages::from(Registry(None));
        assert_eq!(official.universe().url(), "https://packages.typst.org");
    }

    /// A mirror redirects downloads and nothing else: an already-installed
    /// package still comes from the same local directories, so naming one does
    /// not re-download a site's dependencies.
    #[test]
    fn a_mirror_keeps_the_local_package_directories() {
        let mirrored = SystemPackages::from(Registry(Some("https://packages.example.net")));
        let official = SystemPackages::from(Registry(None));
        let path = |store: &SystemPackages| {
            (
                store.data().map(|d| d.path().to_path_buf()),
                store.cache().map(|c| c.path().to_path_buf()),
            )
        };
        assert_eq!(path(&mirrored), path(&official));
    }
}
