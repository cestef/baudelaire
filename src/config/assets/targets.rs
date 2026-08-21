//! `assets { targets { } }`: the browsers a stylesheet is compiled for.

use dispatch_derive::Table;

use crate::config::dispatch::{Block, Section};
use crate::config::vocab::rule;

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

/// Written back as `major.minor.patch`, which is what a config line spells.
impl std::fmt::Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let [_, major, minor, patch] = self.0.to_be_bytes();
        write!(f, "{major}.{minor}.{patch}")
    }
}

impl From<Version> for crate::config::Value {
    fn from(version: Version) -> Self {
        Self::written(version)
    }
}

/// The oldest browser version the CSS must run on, per browser. Naming any
/// turns lightningcss's *transform* on (nesting flattened, prefixes added,
/// colour fallbacks); without one it only minifies.
#[derive(Debug, Clone, Default, Hash, PartialEq, Eq, Table)]
pub struct TargetConfig {
    /// Oldest Android WebView.
    #[key(opt version)]
    pub android: Option<Version>,

    /// Oldest Chrome.
    #[key(opt version)]
    pub chrome: Option<Version>,

    /// Oldest Edge.
    #[key(opt version)]
    pub edge: Option<Version>,

    /// Oldest Firefox.
    #[key(opt version)]
    pub firefox: Option<Version>,

    /// Oldest Internet Explorer.
    #[key(opt version)]
    pub ie: Option<Version>,

    /// Oldest Safari on iOS.
    #[key(opt version)]
    pub ios: Option<Version>,

    /// Oldest Opera.
    #[key(opt version)]
    pub opera: Option<Version>,

    /// Oldest Safari.
    #[key(opt version)]
    pub safari: Option<Version>,

    /// Oldest Samsung Internet.
    #[key(opt version)]
    pub samsung: Option<Version>,
}

impl TargetConfig {
    /// Whether any browser is named, and so whether there is a floor at all.
    pub fn any(&self) -> bool {
        *self != Self::default()
    }
}
