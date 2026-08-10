//! `html { math { } }`: how a page carries the rules its MathML needs.

use crate::config::Named;
use crate::config::dispatch::Kind::Choice;
use crate::config::dispatch::{Block, Section};
use crate::config::node::NodeExt;
use crate::config::value::ValueExt;

/// Where the CSS that MathML output depends on lives.
///
/// typst-html injects it into the `<head>` of every page holding an equation,
/// which is the same block written again on each of them. The three answers a
/// site can give are a served file, that inline block, and nothing at all; they
/// are not degrees of one setting, which is why this is not a switch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum MathStyles {
    /// Lift the rules into one stylesheet under the asset root and link it from
    /// the pages that need it. Cacheable, fingerprinted, and one copy.
    #[default]
    Link,
    /// Leave typst's block where it put it. What every page got before the
    /// stylesheet existed, and what a site wanting no extra request keeps.
    Inline,
    /// Drop the block and serve nothing: the site's own stylesheet states the
    /// rules. For a theme that already bundles them and would otherwise ship
    /// them twice.
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
    /// Where this build's equation rules land: the configured answer, with the
    /// one setting that overrides it applied.
    ///
    /// `html { embed }` makes a page self-contained by inlining what it
    /// references, and for these rules the self-contained form is the block
    /// typst already wrote. Serving a file so that the embed pass can read it
    /// back and inline it is a round trip with the same page at the end of it,
    /// and it puts a file in `dist` that nothing then points at.
    ///
    /// Resolved in one place because both sides ask: the asset pipeline decides
    /// whether to name the file, and the render pass whether to link it. Two
    /// readings of `styles` would be two chances to disagree.
    pub fn of(html: &super::HtmlConfig) -> Self {
        match html.embed && html.math.styles == Self::Link {
            true => Self::Inline,
            false => html.math.styles,
        }
    }

    /// Whether the build writes the stylesheet at all. Asked by the asset
    /// pipeline, which runs before any page has rendered and so cannot know
    /// whether one holds an equation.
    pub fn served(self) -> bool {
        self == Self::Link
    }

    /// Whether typst's inline block is removed from a page's `<head>`.
    pub fn hoisted(self) -> bool {
        self != Self::Inline
    }
}

/// Math output options.
#[derive(Debug, Clone, Default, Hash)]
pub struct MathConfig {
    /// Where the MathML rules live.
    pub styles: MathStyles,
}

impl Section for MathConfig {
    const RULES: Block<Self> = Block(&[(
        "styles",
        Choice(MathStyles::names),
        "Where the CSS that MathML needs lives: a served `link`, typst's `inline` block, or `none`.",
        |c, n, t| {
            c.styles = n.arg(t, 0)?.one::<MathStyles>(t, NodeExt::span(n))?;
            Ok(())
        },
    )]);
}
