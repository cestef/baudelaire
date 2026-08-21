//! `html { math { } }`: how a page carries the rules its MathML needs.

use std::path::PathBuf;

use dispatch_derive::Table;

use crate::config::Named;
use crate::config::dispatch::{Block, Section};
use crate::config::vocab::rule;

/// Where the CSS that MathML output depends on lives.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum MathStyles {
    /// Lift the rules into one stylesheet under the asset root and link it from
    /// the pages that need it.
    #[default]
    Link,
    /// Leave typst's block where it put it.
    Inline,
    /// Drop the block and serve nothing: the site's own stylesheet states the
    /// rules.
    None,
}

impl Named for MathStyles {
    const NAMES: &'static [(&'static str, Self)] = &[
        ("link", Self::Link),
        ("inline", Self::Inline),
        ("none", Self::None),
    ];
}

impl MathStyles {
    /// Where this build's equation rules land: the configured answer, except
    /// that under `html { embed }` the self-contained form is typst's own
    /// inline block, so serving a file would leave one in `dist` that nothing
    /// points at.
    pub fn of(html: &super::HtmlConfig) -> Self {
        if html.embed && html.math.styles == Self::Link {
            Self::Inline
        } else {
            html.math.styles
        }
    }

    /// Whether the build writes the stylesheet at all.
    pub fn served(self) -> bool {
        self == Self::Link
    }

    /// Whether typst's inline block is removed from a page's `<head>`.
    pub fn hoisted(self) -> bool {
        self != Self::Inline
    }
}

#[derive(Debug, Clone, Hash, Table)]
pub struct MathConfig {
    /// Where the CSS that MathML needs lives: a served `link`, typst's `inline` block, or `none`.
    #[key(choice(MathStyles))]
    pub styles: MathStyles,

    /// Where the stylesheet is served from, relative to the asset root.
    #[key(asset)]
    pub path: PathBuf,
}

impl Default for MathConfig {
    fn default() -> Self {
        Self {
            styles: MathStyles::default(),
            path: PathBuf::from("math.css"),
        }
    }
}
