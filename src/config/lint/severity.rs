//! How loud one lint rule is.

use crate::config::Named;

/// What a finding does to the build.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Severity {
    /// The rule does not run.
    Off,
    /// A finding is reported and the build succeeds.
    Warn,
    /// A finding fails the build.
    Error,
}

impl Named for Severity {
    const NAMES: &'static [(&'static str, Self)] = &[
        ("off", Self::Off),
        ("warn", Self::Warn),
        ("error", Self::Error),
    ];
}

/// One rule's configured loudness: a severity it names for itself, or nothing,
/// meaning it follows the site's `strict`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Level(Option<Severity>);

impl Level {
    /// A rule that follows `strict`: the state every rule starts in.
    pub const DEFAULT: Self = Self(None);

    /// A rule turned off by name or by `#false`.
    pub const OFF: Self = Self(Some(Severity::Off));

    /// What a boolean on a rule's line means: `#true` is "on", not `error`, and
    /// `strict` decides how loud that is.
    pub fn flag(on: bool) -> Self {
        if on { Self::DEFAULT } else { Self::OFF }
    }

    pub fn named(severity: Severity) -> Self {
        Self(Some(severity))
    }

    /// Whether the rule runs at all.
    pub fn on(self) -> bool {
        self.severity(false) != Severity::Off
    }

    /// This rule's severity, resolved against the site's `strict`; a rule that
    /// named one keeps it.
    pub fn severity(self, strict: bool) -> Severity {
        self.0.unwrap_or(if strict {
            Severity::Error
        } else {
            Severity::Warn
        })
    }
}
