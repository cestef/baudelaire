//! `assets { targets { } }`: the browsers a stylesheet is compiled for.

use crate::config::dispatch::Kind::Version as Ver;
use crate::config::dispatch::{Block, Section};
use crate::config::node::NodeExt;

/// A browser version, as `major.minor.patch` packed one byte apiece, which is
/// the encoding lightningcss reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Version(pub u32);

impl Version {
    /// A version written as `15`, `15.4` or `15.4.1`, or `None` when it is
    /// neither. Absent components are zero, so `15` is 15.0.0, the first
    /// release of that major.
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
        let no_fourth_component = parts.next().is_none();
        no_fourth_component.then_some(Self(packed))
    }
}

/// The oldest browser version the CSS must run on, per browser. Naming any
/// turns lightningcss's *transform* on (nesting flattened, prefixes added,
/// colour fallbacks); without one it only minifies.
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
    /// Whether any browser is named, and so whether there is a floor at all.
    pub fn any(&self) -> bool {
        *self != Self::default()
    }
}

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
        assert_eq!(Version::parse("300"), None);
        assert_eq!(Version::parse("1.2.3.4"), None);
        assert_eq!(Version::parse("latest"), None);
        assert_eq!(Version::parse(""), None);
    }
}
