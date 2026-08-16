//! Where Typst packages come from: one store behind both a page's `#import`
//! and a theme, with a mirror redirecting the `preview` namespace alone.

use typst_kit::downloader::SystemDownloader;
use typst_kit::packages::{FsPackages, SystemPackages, UniversePackages};

/// The registry the `preview` namespace is downloaded from: a mirror when the
/// site names one, the official one otherwise.
pub struct Registry<'a>(pub Option<&'a str>);

impl From<Registry<'_>> for SystemPackages {
    fn from(Registry(url): Registry<'_>) -> Self {
        let downloader = SystemDownloader::new(super::USER_AGENT);
        match url {
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

    #[test]
    fn a_mirror_replaces_the_registry_url() {
        let mirrored = SystemPackages::from(Registry(Some("https://packages.example.net")));
        assert_eq!(mirrored.universe().url(), "https://packages.example.net");

        let official = SystemPackages::from(Registry(None));
        assert_eq!(official.universe().url(), "https://packages.typst.org");
    }

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
