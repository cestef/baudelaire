//! `lint { }`: post-render checks over the typed DOM.

pub mod budget;
pub mod severity;

use crate::config::dispatch::Kind::Block as Nested;
use crate::config::dispatch::Kind::{Flag, Level as Loud};
use crate::config::dispatch::{Block, Section, Switch};
use crate::config::node::NodeExt;
use crate::config::{BudgetConfig, Level, Named, Severity};

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
    /// The severity a rule that names none takes, exactly as
    /// [`LinkConfig::strict`](crate::config::LinkConfig::strict) does for a broken link.
    pub strict: bool,
    /// Report a heading that skips a level (`h2` straight to `h4`).
    pub headings: Level,
    /// Report an `<img>` carrying no `alt` (an empty one is a decorative image,
    /// and is fine).
    pub alt: Level,
    /// Report an `id` used more than once on one page.
    pub ids: Level,
    /// Report an unknown ARIA role or `aria-*` attribute, and one whose id
    /// reference names nothing on the page.
    pub aria: Level,
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
            headings: Level::DEFAULT,
            alt: Level::DEFAULT,
            ids: Level::DEFAULT,
            aria: Level::DEFAULT,
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
            "Fail the build on a finding instead of warning. A rule naming its own severity keeps it.",
            |c, n, t| {
                c.strict = n.boolean(t, 0)?;
                Ok(())
            },
        ),
        (
            "headings",
            Loud(Severity::names),
            "Report a heading that skips a level, e.g. `h2` straight to `h4`.",
            |c, n, t| {
                c.headings = n.level(t, 0)?;
                Ok(())
            },
        ),
        (
            "alt",
            Loud(Severity::names),
            "Report an image with no `alt` attribute at all (an empty one marks it decorative).",
            |c, n, t| {
                c.alt = n.level(t, 0)?;
                Ok(())
            },
        ),
        (
            "ids",
            Loud(Severity::names),
            "Report an `id` used more than once on a page.",
            |c, n, t| {
                c.ids = n.level(t, 0)?;
                Ok(())
            },
        ),
        (
            "aria",
            Loud(Severity::names),
            "Report an unknown ARIA role or attribute, and one referring to an id that is not there.",
            |c, n, t| {
                c.aria = n.level(t, 0)?;
                Ok(())
            },
        ),
        (
            "budget",
            Nested(BudgetConfig::rows),
            "How many bytes one page may ship.",
            |c, n, t| c.budget.fill(n, t),
        ),
    ]);
}

/// Which lint rule a finding came from, so a report can ask the config how loud
/// that rule is.
///
/// A finding is cached with the page that produced it and its severity is not:
/// the severity is config, and config a site can change between builds without
/// recompiling a page. Resolving it from the rule at report time is what keeps a
/// cache hit reporting what the *current* config asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ruled {
    Headings,
    Alt,
    Ids,
    Aria,
}

impl LintConfig {
    /// How loud `rule` is under this config.
    pub fn severity(&self, rule: Ruled) -> Severity {
        let level = match rule {
            Ruled::Headings => self.headings,
            Ruled::Alt => self.alt,
            Ruled::Ids => self.ids,
            Ruled::Aria => self.aria,
        };
        level.severity(self.strict)
    }
}
