//! A theme from the Typst package store, the same store the compiler resolves
//! `#import "@preview/.."` through. Installing one copies it into the project,
//! which is the difference between this and naming the package in `theme`.

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

    /// A package specifier, `@namespace/name:version`, parsed rather than
    /// pattern-matched so what this claims is what the compiler would resolve.
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
        Local::read(root.path(), parsed.name.to_string(), None, origin.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
