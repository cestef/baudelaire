//! `lint { }`: post-render checks over the typed DOM.

pub mod budget;

use crate::config::BudgetConfig;
use crate::config::dispatch::Kind::Block as Nested;
use crate::config::dispatch::Kind::Flag;
use crate::config::dispatch::{Block, Section, Switch};
use crate::config::node::NodeExt;

/// Linting of the built pages: which rules run over the typed DOM, how loud a
/// finding is, and how many bytes a page may weigh.
///
/// Off until a `lint { }` block says otherwise. A lint is a claim about what the
/// site *should* look like, and inventing one for a site that never asked is the
/// same opinionated-default problem as a generated page nobody wanted.
#[derive(Debug, Clone, Hash)]
pub struct LintConfig {
    /// Whether the DOM lint pass runs at all; flipped by the block's presence,
    /// and back off again by `lint #false`.
    pub enabled: bool,
    /// Fail the build on a finding instead of warning, exactly as
    /// [`LinkConfig::strict`](crate::config::LinkConfig::strict) does for a broken link.
    pub strict: bool,
    /// Report a heading that skips a level (`h2` straight to `h4`).
    pub headings: bool,
    /// Report an `<img>` carrying no `alt` (an empty one is a decorative image,
    /// and is fine).
    pub alt: bool,
    /// Report an `id` used more than once on one page.
    pub ids: bool,
    /// Report an unknown ARIA role or `aria-*` attribute, and one whose id
    /// reference names nothing on the page.
    pub aria: bool,
    /// How many bytes a single page may ship.
    pub budget: BudgetConfig,
}

impl Default for LintConfig {
    fn default() -> Self {
        // opt-in as a whole: the presence of a `lint { }` block flips `enabled`.
        // Every rule is then on, because a block that turns nothing on is not
        // what an author writing one meant; each is switched off by name.
        // `strict` follows `links`' lenient half rather than its strict one: a
        // broken link is a certainty, whereas a missing `alt` is a judgement
        // about content this tool did not write.
        Self {
            enabled: false,
            strict: false,
            headings: true,
            alt: true,
            ids: true,
            aria: true,
            budget: BudgetConfig::default(),
        }
    }
}

/// The `lint { .. }` section: the rules run over each built page's DOM, and how
/// loud a finding is. The block's presence is what turns linting on.
impl Section for LintConfig {
    const SWITCH: Option<Switch<Self>> = Some(|c, on| c.enabled = on);

    const RULES: Block<Self> = Block(&[
        (
            "strict",
            Flag,
            "Fail the build on a finding instead of warning.",
            |c, n, t| {
                c.strict = n.boolean(t, 0)?;
                Ok(())
            },
        ),
        (
            "headings",
            Flag,
            "Report a heading that skips a level, e.g. `h2` straight to `h4`.",
            |c, n, t| {
                c.headings = n.boolean(t, 0)?;
                Ok(())
            },
        ),
        (
            "alt",
            Flag,
            "Report an image with no `alt` attribute at all (an empty one marks it decorative).",
            |c, n, t| {
                c.alt = n.boolean(t, 0)?;
                Ok(())
            },
        ),
        (
            "ids",
            Flag,
            "Report an `id` used more than once on a page.",
            |c, n, t| {
                c.ids = n.boolean(t, 0)?;
                Ok(())
            },
        ),
        (
            "aria",
            Flag,
            "Report an unknown ARIA role or attribute, and one referring to an id that is not there.",
            |c, n, t| {
                c.aria = n.boolean(t, 0)?;
                Ok(())
            },
        ),
        (
            "budget",
            Nested(BudgetConfig::rows),
            "How many bytes one page may ship. Exceeding a budget always fails the build.",
            |c, n, t| c.budget.fill(n, t),
        ),
    ]);
}
