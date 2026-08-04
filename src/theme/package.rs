//! A theme from the Typst package store.
//!
//! The same store the compiler resolves `#import "@preview/.."` through, so a
//! theme published as a package is fetched by the machinery already here: one
//! download, cached across projects, pinned by the exact version in the spec.
//!
//! Installing one *copies* it into the project, which is the difference between
//! this and naming the package in `theme`. Naming it leaves the files in the
//! store, read-only and shared; copying makes them the project's, to edit and
//! commit. Both work; this is for when the theme is a starting point rather
//! than a dependency.

use typst::syntax::package::PackageSpec;
use typst_kit::packages::SystemPackages;

use super::local::Local;
use super::source::{Fetched, Fetching, Origin, Source};
use crate::error::{Result, ThemeError};
use crate::world::Registry;

/// The Typst package store.
pub struct Store;

impl Source for Store {
    fn name(&self) -> &'static str {
        "package"
    }

    /// A package specifier, which is the one spec shape Typst itself defines:
    /// `@namespace/name:version`. Parsed here rather than pattern-matched, so
    /// what this claims is exactly what the compiler would resolve.
    fn parse(&self, spec: &str) -> Option<Origin> {
        spec.parse::<PackageSpec>()
            .ok()
            .map(|spec| Origin::Package {
                spec: spec.to_string(),
            })
    }

    fn owns(&self, origin: &Origin) -> bool {
        matches!(origin, Origin::Package { .. })
    }

    fn fetch(&self, origin: &Origin, cx: &Fetching) -> Result<Fetched> {
        let Origin::Package { spec } = origin else {
            return Err(ThemeError::unsupported(origin.label()).into());
        };
        let parsed: PackageSpec = spec
            .parse()
            .map_err(|why: typst::ecow::EcoString| ThemeError::spec(spec, why))?;
        let root = SystemPackages::from(Registry(cx.registry.as_deref()))
            .obtain(&parsed)
            .map_err(|why| ThemeError::unavailable(spec, why))?;
        // The package's own name, not the last segment of the spec, because the
        // version is part of that segment.
        Local::read(root.path(), parsed.name.to_string(), None, origin.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A specifier is claimed, and nothing else is: `@` alone is not a package,
    /// and a bare name belongs to the shelf.
    #[test]
    fn a_package_is_claimed_by_its_specifier() {
        assert_eq!(
            Store.parse("@preview/plume:1.0.0"),
            Some(Origin::Package {
                spec: "@preview/plume:1.0.0".to_owned()
            })
        );
        assert!(Store.parse("@preview/plume").is_none(), "no version");
        assert!(Store.parse("plume").is_none());
        assert!(Store.parse("./plume").is_none());
    }
}
