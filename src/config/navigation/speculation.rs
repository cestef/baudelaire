//! `navigation { speculation { } }`: browser-native prefetch hints.

use crate::config::Named;
use crate::config::dispatch::Kind::Choice;
use crate::config::dispatch::{Block, Section};
use crate::config::node::NodeExt;
use crate::config::value::ValueExt;

/// Browser-native navigation hints: a `<script type="speculationrules">` telling
/// the browser to fetch, or fully render, an internal link's target before it is
/// clicked. Enabled by the presence of a `navigation { speculation { .. } }`
/// block.
///
/// The zero-JavaScript neighbour of [`SpaConfig`](crate::config::SpaConfig): the browser does the work, so
/// nothing has to be shipped, mounted, or maintained. Unsupported browsers
/// ignore the script.
#[derive(Debug, Clone, Hash)]
pub struct SpeculationConfig {
    /// Whether to emit the rules.
    pub enabled: bool,
    /// How eagerly to fetch a link's target (cheap: bytes only).
    pub prefetch: Eagerness,
    /// How eagerly to render it in full (expensive: a hidden page, its scripts
    /// running), so the click paints instantly.
    pub prerender: Eagerness,
}

/// How eagerly the browser should act on a speculation rule, from the API's own
/// scale, plus a [`Eagerness::None`] that emits no rule at all for that action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Eagerness {
    /// Emit no rule: this action is off.
    #[default]
    None,
    /// On pointer-down: the last moment before a navigation.
    Conservative,
    /// On hover, roughly, once intent looks real.
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
        // opt-in like its neighbours. When the block is present but silent:
        // prefetch on hover (cheap, near-certain to be used) and no prerender,
        // which costs a full hidden page render and runs the target's scripts.
        Self {
            enabled: false,
            prefetch: Eagerness::Moderate,
            prerender: Eagerness::None,
        }
    }
}

/// The `speculation { prefetch ..; prerender .. }` block. Its presence enables
/// the navigation hints.
impl Section for SpeculationConfig {
    const RULES: Block<Self> = Block(&[
        (
            "prefetch",
            Choice(Eagerness::names),
            "How eagerly the browser fetches a linked page.",
            |c, n, t| {
                c.prefetch = n.arg(t, 0)?.one::<Eagerness>(t, NodeExt::span(n))?;
                Ok(())
            },
        ),
        (
            "prerender",
            Choice(Eagerness::names),
            "How eagerly it renders one ahead of the click.",
            |c, n, t| {
                c.prerender = n.arg(t, 0)?.one::<Eagerness>(t, NodeExt::span(n))?;
                Ok(())
            },
        ),
    ]);

    fn enable(&mut self) -> bool {
        self.enabled = true;
        true
    }
}
