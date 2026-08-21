//! `check { }`: what the build verifies about the pages it produced: their
//! links, their markup, and their weight.

pub mod budget;
pub mod external;
pub mod headings;
pub mod rule;
pub mod severity;
pub mod snippets;

use dispatch_derive::Table;

use crate::config::Value;
use crate::config::check::rule::{Rule, Ruled};
use crate::config::dispatch::Kind::Lines;
use crate::config::dispatch::{Attributed, Block, Section};
use crate::config::node::NodeExt;
use crate::config::vocab::rule;
use crate::config::{
    BudgetConfig, ExternalConfig, HeadingConfig, Level, Named, Severity, SnippetConfig,
};

/// What the build verifies about the pages it produced.
///
/// The markup rules are off until a `check { }` block says otherwise; the link
/// rules answer for themselves, so a block turned off still carries them.
#[derive(Debug, Clone, Hash, Table)]
#[table(hook(switch = enabled))]
pub struct CheckConfig {
    /// Whether the DOM lint pass runs at all.
    pub enabled: bool,

    /// Fail the build on a finding instead of warning. A rule naming its own severity keeps it.
    #[key(flag)]
    pub strict: bool,

    /// Report an internal link that resolves to no page. `error` by default, so a broken link fails the build.
    #[key(level)]
    pub links: Level,

    /// Check outbound `http(s)` links over the network. Its presence turns it on; `#false` turns it off again.
    #[key(nested(ExternalConfig))]
    pub external: ExternalConfig,

    /// Report the pages nothing links to, counting `any` page's links or only those an author wrote.
    ///
    /// `None` leaves the report off.
    #[key(opt choice(Linked))]
    pub orphans: Option<Linked>,

    /// Report a heading that skips a level, e.g. `h2` straight to `h4`. `headings "warn"` is `headings { level "warn" }`.
    ///
    /// Also carries the level a page's own outline opens at.
    #[key(name = Rule::Headings.key(), shorthand(HeadingConfig, "level"))]
    pub headings: HeadingConfig,

    /// Report an image with no `alt` attribute at all (an empty one marks it decorative).
    #[key(name = Rule::Alt.key(), level)]
    pub alt: Level,

    /// Report an `id` used more than once on a page.
    #[key(name = Rule::Ids.key(), level)]
    pub ids: Level,

    /// Report an unknown ARIA role or attribute, and one referring to an id that is not there.
    #[key(name = Rule::Aria.key(), level)]
    pub aria: Level,

    /// How many bytes one page may ship.
    #[key(nested(BudgetConfig))]
    pub budget: BudgetConfig,

    /// One line per code fence language, saying how a snippet of it is checked.
    ///
    /// In the order the config declares them.
    #[key(name = Rule::Snippets.key(), custom(
        Lines(SnippetConfig::rows),
        |c: &Self| Value::each(&c.snippets, Attributed::values),
        |c: &mut Self, n: &kdl::KdlNode, t: &str| {
            c.snippets = n.unique(t, "snippet language", SnippetConfig::item)?;
            Ok(())
        },
    ))]
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
