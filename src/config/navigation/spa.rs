//! `navigation { spa { } }`: client-side navigation over the built files.

use crate::config::Named;
use crate::config::dispatch::Kind::{Choice, Text};
use crate::config::dispatch::{Block, Section, Switch};
use crate::config::node::NodeExt;
use crate::config::value::ValueExt;

/// Client-side navigation over the ordinary multi-file output: a runtime
/// intercepts internal link clicks, fetches the target page, and swaps one
/// container instead of reloading.
#[derive(Debug, Clone, Hash)]
pub struct SpaConfig {
    /// Whether to emit the navigation runtime.
    pub enabled: bool,
    /// CSS selector of the element swapped on navigation; everything outside it
    /// survives untouched, so it must be the one element whose contents differ
    /// between pages.
    pub root: String,
    pub prefetch: Prefetch,
}

/// When the router warms a link's target.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Prefetch {
    None,
    /// On pointer-over or keyboard focus.
    #[default]
    Hover,
    /// As soon as the link scrolls into view.
    Visible,
}

impl Named for Prefetch {
    const NAMES: &'static [(&'static str, Self)] = &[
        ("none", Self::None),
        ("hover", Self::Hover),
        ("visible", Self::Visible),
    ];
}

impl Default for SpaConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            root: "main".into(),
            prefetch: Prefetch::default(),
        }
    }
}

impl Section for SpaConfig {
    const SWITCH: Option<Switch<Self>> = Some(|c, on| c.enabled = on);

    const RULES: Block<Self> = Block(&[
        (
            "root",
            Text,
            "The element swapped out on navigation.",
            |c, n, t| {
                c.root = n.string(t, 0)?;
                Ok(())
            },
        ),
        (
            "prefetch",
            Choice(Prefetch::names),
            "When to fetch a page ahead of the click.",
            |c, n, t| {
                c.prefetch = n.arg(t, 0)?.one::<Prefetch>(t, NodeExt::span(n))?;
                Ok(())
            },
        ),
    ]);
}
