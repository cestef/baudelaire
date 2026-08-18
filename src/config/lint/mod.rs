//! `lint { }`: post-render checks over the typed DOM.

pub mod budget;
pub mod rule;
pub mod severity;
pub mod snippets;

use crate::config::dispatch::Kind::Block as Nested;
use crate::config::dispatch::Kind::{Flag, Level as Loud, Lines};
use crate::config::dispatch::{Attributed, Block, Section, Switch};
use crate::config::lint::rule::{Rule, Ruled};
use crate::config::node::NodeExt;
use crate::config::{BudgetConfig, Level, Named, Severity, SnippetConfig};

/// Linting of the built pages: which rules run over the typed DOM, how loud a
/// finding is, and how many bytes a page may weigh. Off until a `lint { }`
/// block says otherwise.
#[derive(Debug, Clone, Hash)]
pub struct LintConfig {
    /// Whether the DOM lint pass runs at all.
    pub enabled: bool,
    /// The severity a rule that names none takes.
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
    /// How a code fence is checked, one entry per language it may claim, in the
    /// order the config declares them.
    pub snippets: Vec<(String, SnippetConfig)>,
}

impl Default for LintConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            strict: false,
            headings: Level::DEFAULT,
            alt: Level::DEFAULT,
            ids: Level::DEFAULT,
            aria: Level::DEFAULT,
            budget: BudgetConfig::default(),
            snippets: Vec::new(),
        }
    }
}

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
            Rule::Headings.key(),
            Loud(Severity::names),
            "Report a heading that skips a level, e.g. `h2` straight to `h4`.",
            |c, n, t| {
                c.headings = n.level(t, 0)?;
                Ok(())
            },
        ),
        (
            Rule::Alt.key(),
            Loud(Severity::names),
            "Report an image with no `alt` attribute at all (an empty one marks it decorative).",
            |c, n, t| {
                c.alt = n.level(t, 0)?;
                Ok(())
            },
        ),
        (
            Rule::Ids.key(),
            Loud(Severity::names),
            "Report an `id` used more than once on a page.",
            |c, n, t| {
                c.ids = n.level(t, 0)?;
                Ok(())
            },
        ),
        (
            Rule::Aria.key(),
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
        (
            Rule::Snippets.key(),
            Lines(SnippetConfig::rows),
            "One line per code fence language, saying how a snippet of it is checked.",
            |c, n, t| {
                c.snippets = n.unique(t, "snippet language", SnippetConfig::item)?;
                Ok(())
            },
        ),
    ]);
}

impl LintConfig {
    /// How loud `rule` is under this config. A snippet language nothing
    /// configures is [`Level::OFF`]: the finding it came from was cached under
    /// a config that named it, and this one does not.
    pub fn severity(&self, rule: &Ruled) -> Severity {
        let level = match (rule.rule, rule.lang.as_deref()) {
            (Rule::Headings, _) => self.headings,
            (Rule::Alt, _) => self.alt,
            (Rule::Ids, _) => self.ids,
            (Rule::Aria, _) => self.aria,
            (Rule::Snippets, Some(lang)) => {
                self.snippet(lang).map_or(Level::OFF, |rule| rule.level)
            }
            (Rule::Snippets, None) => Level::OFF,
        };
        level.severity(self.strict)
    }

    /// How fences of `lang` are checked, or `None` for a language no line names.
    pub fn snippet(&self, lang: &str) -> Option<&SnippetConfig> {
        self.snippets
            .iter()
            .find(|(named, _)| named == lang)
            .map(|(_, rule)| rule)
    }
}
