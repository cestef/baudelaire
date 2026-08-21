//! `navigation { spa { } }`: client-side navigation over the built files.

use dispatch_derive::Table;

use crate::config::Named;
use crate::config::dispatch::{Block, Section};
use crate::config::vocab::rule;

/// Client-side navigation over the ordinary multi-file output: a runtime
/// intercepts internal link clicks, fetches the target page, and swaps one
/// container instead of reloading.
#[derive(Debug, Clone, Hash, Table)]
#[table(hook(switch = enabled))]
pub struct SpaConfig {
    /// Whether to emit the navigation runtime.
    pub enabled: bool,

    /// The element swapped out on navigation.
    ///
    /// A CSS selector. Everything outside it survives untouched, so it must be
    /// the one element whose contents differ between pages.
    #[key(text)]
    pub root: String,

    /// When to fetch a page ahead of the click.
    #[key(choice(Prefetch))]
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
