//! `navigation { speculation { } }`: browser-native prefetch hints.

use dispatch_derive::Table;

use crate::config::Named;
use crate::config::dispatch::{Block, Section};
use crate::config::vocab::rule;

/// Browser-native navigation hints: a `<script type="speculationrules">`
/// telling the browser to fetch, or fully render, an internal link's target
/// before it is clicked.
#[derive(Debug, Clone, Hash, Table)]
#[table(hook(switch = enabled))]
pub struct SpeculationConfig {
    /// Whether to emit the rules.
    pub enabled: bool,

    /// How eagerly the browser fetches a linked page.
    ///
    /// Cheap: bytes only.
    #[key(choice(Eagerness))]
    pub prefetch: Eagerness,

    /// How eagerly it renders one ahead of the click.
    ///
    /// Expensive: a hidden page, its scripts running.
    #[key(choice(Eagerness))]
    pub prerender: Eagerness,
}

/// How eagerly the browser should act on a speculation rule, from the API's own
/// scale, plus a [`Eagerness::None`] that emits no rule at all for that action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Eagerness {
    /// Emit no rule: this action is off.
    #[default]
    None,
    /// On pointer-down.
    Conservative,
    /// On hover, roughly.
    Moderate,
    /// As soon as a link looks like a plausible next step.
    Eager,
    /// At once, for every matching link on the page.
    Immediate,
}

impl Named for Eagerness {
    const NAMES: &'static [(&'static str, Self)] = &[
        ("none", Self::None),
        ("conservative", Self::Conservative),
        ("moderate", Self::Moderate),
        ("eager", Self::Eager),
        ("immediate", Self::Immediate),
    ];
}

impl Default for SpeculationConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            prefetch: Eagerness::Moderate,
            prerender: Eagerness::None,
        }
    }
}
