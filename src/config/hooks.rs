//! `hooks { }`: external commands run around the build.

use crate::config::dispatch::Kind::Texts;
use crate::config::dispatch::{Block, Section};
use crate::config::node::NodeExt;

/// External command hooks. Each command runs through the system shell in the
/// project root: the escape hatch for tools baudelaire does not embed
/// (Tailwind, PostCSS, Pagefind, image optimizers, deploy scripts).
#[derive(Debug, Clone, Hash, Default)]
pub struct HooksConfig {
    /// Commands run before the build (before the asset pipeline), so any files
    /// they generate into `assets/` are picked up and fingerprinted.
    pub before: Vec<String>,
    /// Commands run after the site is written to `dist`.
    pub after: Vec<String>,
}

/// Each list replaces (never appends) so a profile overriding `before` leaves
/// `after` inherited from the base: same whole-value replacement as every other
/// list field.
impl Section for HooksConfig {
    const RULES: Block<Self> = Block(&[
        (
            "before",
            Texts,
            "Commands run before the asset pipeline, so what they generate is picked up.",
            |c, n, t| {
                c.before = n.words(t)?;
                Ok(())
            },
        ),
        (
            "after",
            Texts,
            "Commands run once the output directory is written.",
            |c, n, t| {
                c.after = n.words(t)?;
                Ok(())
            },
        ),
    ]);
}
