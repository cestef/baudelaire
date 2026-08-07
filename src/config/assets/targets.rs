//! `assets { targets { } }`: the browsers a stylesheet is compiled for.

use crate::config::dispatch::Kind::Version as Ver;
use crate::config::dispatch::{Block, Section};
use crate::config::node::NodeExt;

/// A browser version, as `major.minor.patch` packed one byte apiece.
///
/// The encoding lightningcss reads, held here so the config owns the parsing and
/// the pipeline owns nothing but the handover. A newtype rather than a bare
/// `u32` because the packing is the whole content of the type: a plain integer
/// field would invite `safari: Some(15)`, which is version 0.0.15.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Version(pub u32);

impl Version {
    /// A version written as `15`, `15.4` or `15.4.1`, or `None` when it is
    /// neither. Absent components are zero, so `15` is 15.0.0: the *first*
    /// release of that major, which is what naming a browser floor means.
    pub fn parse(text: &str) -> Option<Self> {
        let mut parts = text.trim().split('.');
        let mut packed = 0u32;
        for shift in [16, 8, 0] {
            let component = match parts.next() {
                Some(part) => part.parse::<u8>().ok()?,
                None => 0,
            };
            packed |= u32::from(component) << shift;
        }
        // A fourth component is not a version this encoding can hold, and
        // dropping it silently would compile for a browser nobody named.
        parts.next().is_none().then_some(Self(packed))
    }
}

/// The oldest browser version the CSS must run on, per browser.
///
/// Naming any of these turns lightningcss's *transform* on: nesting is flattened,
/// vendor prefixes are added, and modern colour syntaxes get fallbacks, each only
/// where a named browser needs it. Without them lightningcss only minifies, which
/// is a fraction of what it is for, and a site using CSS nesting shipped it
/// unflattened to browsers that cannot read it.
///
/// A browser left unnamed is not a constraint: it is not "support every version",
/// it is "this browser is not what the floor is set by".
#[derive(Debug, Clone, Default, Hash, PartialEq, Eq)]
pub struct TargetConfig {
    pub android: Option<Version>,
    pub chrome: Option<Version>,
    pub edge: Option<Version>,
    pub firefox: Option<Version>,
    pub ie: Option<Version>,
    pub ios: Option<Version>,
    pub opera: Option<Version>,
    pub safari: Option<Version>,
    pub samsung: Option<Version>,
}

impl TargetConfig {
    /// Whether any browser is named, and so whether there is a floor to compile
    /// against at all.
    pub fn any(&self) -> bool {
        *self != Self::default()
    }
}

/// One row per browser lightningcss knows. Written out rather than derived from
/// a name table because [`Block`] is a constant: the table *is* the nine rows,
/// and each names the field it fills.
impl Section for TargetConfig {
    const RULES: Block<Self> = Block(&[
        ("android", Ver, "Oldest Android WebView.", |c, n, t| {
            c.android = Some(n.version(t, 0)?);
            Ok(())
        }),
        ("chrome", Ver, "Oldest Chrome.", |c, n, t| {
            c.chrome = Some(n.version(t, 0)?);
            Ok(())
        }),
        ("edge", Ver, "Oldest Edge.", |c, n, t| {
            c.edge = Some(n.version(t, 0)?);
            Ok(())
        }),
        ("firefox", Ver, "Oldest Firefox.", |c, n, t| {
            c.firefox = Some(n.version(t, 0)?);
            Ok(())
        }),
        ("ie", Ver, "Oldest Internet Explorer.", |c, n, t| {
            c.ie = Some(n.version(t, 0)?);
            Ok(())
        }),
        ("ios", Ver, "Oldest Safari on iOS.", |c, n, t| {
            c.ios = Some(n.version(t, 0)?);
            Ok(())
        }),
        ("opera", Ver, "Oldest Opera.", |c, n, t| {
            c.opera = Some(n.version(t, 0)?);
            Ok(())
        }),
        ("safari", Ver, "Oldest Safari.", |c, n, t| {
            c.safari = Some(n.version(t, 0)?);
            Ok(())
        }),
        ("samsung", Ver, "Oldest Samsung Internet.", |c, n, t| {
            c.samsung = Some(n.version(t, 0)?);
            Ok(())
        }),
    ]);
}

#[cfg(test)]
mod tests {
    use super::Version;

    #[test]
    fn a_version_packs_one_byte_per_component() {
        assert_eq!(Version::parse("15"), Some(Version(15 << 16)));
        assert_eq!(Version::parse("15.4"), Some(Version((15 << 16) | (4 << 8))));
        assert_eq!(
            Version::parse("15.4.1"),
            Some(Version((15 << 16) | (4 << 8) | 1))
        );
        assert_eq!(Version::parse(" 120 "), Some(Version(120 << 16)));
    }

    #[test]
    fn a_version_that_does_not_fit_the_encoding_is_refused() {
        // A component past one byte, a fourth component, and text.
        assert_eq!(Version::parse("300"), None);
        assert_eq!(Version::parse("1.2.3.4"), None);
        assert_eq!(Version::parse("latest"), None);
        assert_eq!(Version::parse(""), None);
    }
}
