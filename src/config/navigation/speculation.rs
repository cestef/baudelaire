//! `navigation { speculation { } }`: browser-native prefetch hints.

use crate::config::Named;
use crate::config::Value;
use crate::config::dispatch::Kind::Choice;
use crate::config::dispatch::{Block, Section, Switch};
use crate::config::node::NodeExt;
use crate::config::value::ValueExt;

/// Browser-native navigation hints: a `<script type="speculationrules">`
/// telling the browser to fetch, or fully render, an internal link's target
/// before it is clicked.
#[derive(Debug, Clone, Hash)]
pub struct SpeculationConfig {
    /// Whether to emit the rules.
    pub enabled: bool,
    /// How eagerly to fetch a link's target (cheap: bytes only).
    pub prefetch: Eagerness,
    /// How eagerly to render it in full (expensive: a hidden page, its scripts
    /// running).
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

impl Section for SpeculationConfig {
    const SWITCH: Option<Switch<Self>> = Some(Switch {
        set: |c, on| c.enabled = on,
        on: |c| c.enabled,
    });

    const RULES: Block<Self> = Block(&[
        (
            "prefetch",
            Choice(Eagerness::names),
            "How eagerly the browser fetches a linked page.",
            |c| Value::named(c.prefetch),
            |c, n, t| {
                c.prefetch = n.arg(t, 0)?.one::<Eagerness>(t, NodeExt::span(n))?;
                Ok(())
            },
        ),
        (
            "prerender",
            Choice(Eagerness::names),
            "How eagerly it renders one ahead of the click.",
            |c| Value::named(c.prerender),
            |c, n, t| {
                c.prerender = n.arg(t, 0)?.one::<Eagerness>(t, NodeExt::span(n))?;
                Ok(())
            },
        ),
    ]);
}
