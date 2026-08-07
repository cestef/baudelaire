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
///
/// Three states rather than two, because "on" and "on, and always fatal" are
/// different claims and a site needs both: `lint { strict }` says every finding
/// is an error, and a site that wants exactly one rule exempted has nowhere to
/// say so if a rule can only be on or off. The unset state is what makes
/// `strict` mean anything at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Level(Option<Severity>);

impl Level {
    /// A rule that follows `strict`: the state every rule starts in.
    pub const DEFAULT: Self = Self(None);

    /// A rule turned off by name or by `#false`.
    pub const OFF: Self = Self(Some(Severity::Off));

    /// What a boolean on a rule's line means. `#true` is not `error`: it is
    /// "on", which is what the key has always meant, and `strict` decides how
    /// loud that is.
    pub fn flag(on: bool) -> Self {
        match on {
            true => Self::DEFAULT,
            false => Self::OFF,
        }
    }

    /// A rule that names its own severity.
    pub fn named(severity: Severity) -> Self {
        Self(Some(severity))
    }

    /// Whether the rule runs at all: what the render pass asks, since a rule
    /// that is only going to warn still has to find something to warn about.
    pub fn on(self) -> bool {
        self.severity(false) != Severity::Off
    }

    /// This rule's severity, resolved against the site's `strict`. A rule that
    /// named one keeps it, which is the point: `strict` is a default, not an
    /// override.
    pub fn severity(self, strict: bool) -> Severity {
        self.0.unwrap_or(match strict {
            true => Severity::Error,
            false => Severity::Warn,
        })
    }
}
