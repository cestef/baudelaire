//! `check { }`: what the build verifies about the pages it produced: their
//! links, their markup, and their weight.

pub mod budget;
pub mod external;
pub mod headings;
pub mod rule;
pub mod severity;
pub mod snippets;

use crate::config::Value;
use crate::config::check::rule::{Rule, Ruled};
use crate::config::dispatch::Kind::Block as Nested;
use crate::config::dispatch::Kind::{Choice, Flag, Level as Loud, Lines};
use crate::config::dispatch::{Attributed, Block, Section, Switch};
use crate::config::node::NodeExt;
use crate::config::value::ValueExt;
use crate::config::{
    BudgetConfig, ExternalConfig, HeadingConfig, Level, Named, Severity, SnippetConfig,
};

/// What the build verifies about the pages it produced.
///
/// The markup rules are off until a `check { }` block says otherwise; the link
/// rules answer for themselves, so a block turned off still carries them.
#[derive(Debug, Clone, Hash)]
pub struct CheckConfig {
    /// Whether the DOM lint pass runs at all.
    pub enabled: bool,
    /// The severity a rule that names none takes.
    pub strict: bool,
    /// How loud an internal link resolving to nothing is.
    pub links: Level,
    pub external: ExternalConfig,
    /// Report the pages nothing links to, and what counts as a link. `None`
    /// leaves the report off.
    pub orphans: Option<Linked>,
    /// Report a heading that skips a level (`h2` straight to `h4`), and the
    /// level a page's own outline opens at.
    pub headings: HeadingConfig,
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

/// What counts as pointing at a page, for the orphan report. A layout's own
/// links never count under either.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Linked {
    /// Any page's link, a generated index or term page included.
    #[default]
    Any,
    /// Only a link on a page an author wrote.
    Authored,
}

impl Named for Linked {
    const NAMES: &'static [(&'static str, Self)] =
        &[("any", Self::Any), ("authored", Self::Authored)];
}

impl Linked {
    /// Whether a link on this page counts. `generated` is whether the build
    /// wrote the page rather than an author.
    pub fn counts(self, generated: bool) -> bool {
        !generated || self == Self::Any
    }
}

impl Default for CheckConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            strict: false,
            links: Level::named(Severity::Error),
            external: ExternalConfig::default(),
            orphans: None,
            headings: HeadingConfig::default(),
            alt: Level::DEFAULT,
            ids: Level::DEFAULT,
            aria: Level::DEFAULT,
            budget: BudgetConfig::default(),
            snippets: Vec::new(),
        }
    }
}

impl Section for CheckConfig {
    const SWITCH: Option<Switch<Self>> = Some(Switch {
        set: |c, on| c.enabled = on,
        on: |c| c.enabled,
    });

    const RULES: Block<Self> = Block(&[
        (
            "strict",
            Flag,
            "Fail the build on a finding instead of warning. A rule naming its own severity keeps it.",
            |c| c.strict.into(),
            |c, n, t| {
                c.strict = n.boolean(t, 0)?;
                Ok(())
            },
        ),
        (
            "links",
            Loud(Severity::names),
            "Report an internal link that resolves to no page. `error` by default, so a broken link fails the build.",
            |c| c.links.into(),
            |c, n, t| {
                c.links = n.level(t, 0)?;
                Ok(())
            },
        ),
        (
            "external",
            Nested(ExternalConfig::rows),
            "Check outbound `http(s)` links over the network. Its presence turns it on; `#false` turns it off again.",
            |c| c.external.values(),
            |c, n, t| c.external.fill(n, t),
        ),
        (
            "orphans",
            Choice(Linked::names),
            "Report the pages nothing links to, counting `any` page's links or only those an author wrote.",
            |c| c.orphans.map(Value::named).into(),
            |c, n, t| {
                c.orphans = Some(n.arg(t, 0)?.one::<Linked>(t, NodeExt::span(n))?);
                Ok(())
            },
        ),
        (
            Rule::Headings.key(),
            Nested(HeadingConfig::rows),
            "Report a heading that skips a level, e.g. `h2` straight to `h4`. `headings \"warn\"` is `headings { level \"warn\" }`.",
            |c| c.headings.values(),
            |c, n, t| c.headings.shorthand(n, t, "level"),
        ),
        (
            Rule::Alt.key(),
            Loud(Severity::names),
            "Report an image with no `alt` attribute at all (an empty one marks it decorative).",
            |c| c.alt.into(),
            |c, n, t| {
                c.alt = n.level(t, 0)?;
                Ok(())
            },
        ),
        (
            Rule::Ids.key(),
            Loud(Severity::names),
            "Report an `id` used more than once on a page.",
            |c| c.ids.into(),
            |c, n, t| {
                c.ids = n.level(t, 0)?;
                Ok(())
            },
        ),
        (
            Rule::Aria.key(),
            Loud(Severity::names),
            "Report an unknown ARIA role or attribute, and one referring to an id that is not there.",
            |c| c.aria.into(),
            |c, n, t| {
                c.aria = n.level(t, 0)?;
                Ok(())
            },
        ),
        (
            "budget",
            Nested(BudgetConfig::rows),
            "How many bytes one page may ship.",
            |c| c.budget.values(),
            |c, n, t| c.budget.fill(n, t),
        ),
        (
            Rule::Snippets.key(),
            Lines(SnippetConfig::rows),
            "One line per code fence language, saying how a snippet of it is checked.",
            |c| Value::each(&c.snippets, Attributed::values),
            |c, n, t| {
                c.snippets = n.unique(t, "snippet language", SnippetConfig::item)?;
                Ok(())
            },
        ),
    ]);
}

impl CheckConfig {
    /// How loud `rule` is under this config. A snippet language nothing
    /// configures is [`Level::OFF`]: the finding it came from was cached under
    /// a config that named it, and this one does not.
    pub fn severity(&self, rule: &Ruled) -> Severity {
        let level = match (rule.rule, rule.lang.as_deref()) {
            (Rule::Headings, _) => self.headings.level,
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
