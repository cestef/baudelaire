//! `lint { snippets { } }`: how the code fences of one language are checked.

use kdl::KdlNode;

use crate::config::Level;
use crate::config::dispatch::Kind::Text;
use crate::config::dispatch::{Attributed, Attrs};
use crate::config::node::NodeExt;
use crate::config::value::ValueExt;
use crate::error::{ConfigError, ConfigErrorKind, Result};
use crate::render::lint::snippet::parser;
use crate::ui::markup;

/// How the fences claiming one language are checked.
#[derive(Debug, Clone, Hash)]
pub struct SnippetConfig {
    /// How loud a finding in one of them is.
    pub level: Level,
    /// The command that checks one snippet, `{file}` standing for the file it
    /// is written out to and `{lang}` for the language it claimed. `None`
    /// checks it with this build's parser for that language.
    pub run: Option<String>,
    /// The line prefix that keeps a line out of the page while leaving it in
    /// what the checker reads, for the context a fragment needs to stand on its
    /// own. `None` is a language whose fences are shown entire.
    pub hidden: Option<String>,
}

impl SnippetConfig {
    /// One `kdl "warn" run=".."` line, keyed by the fence language it checks.
    pub(crate) fn item(node: &KdlNode, text: &str) -> Result<(String, Self)> {
        let lang = node.name().value().to_owned();
        let mut rule = Self {
            level: node.level(text, 0)?,
            run: None,
            hidden: None,
        };
        rule.read(node, text)?;
        rule.check(&lang, node, text)?;
        Ok((lang, rule))
    }

    /// Refuse a language this build can neither parse nor be told how to check:
    /// it would parse, check nothing, and leave a site believing its fences are
    /// looked at.
    fn check(&self, lang: &str, node: &KdlNode, text: &str) -> Result<()> {
        if self.run.is_some() || !self.level.on() || parser::of(lang).is_some() {
            return Ok(());
        }
        Err(ConfigError::at(
            text,
            ConfigErrorKind::NoSnippetChecker {
                lang: lang.to_owned(),
                help: markup!(
                    "give it a command, as in `{} run=\"..\"`, or name a language \
                     this build parses itself: {}",
                    lang,
                    parser::langs().join(", ")
                ),
            },
            NodeExt::span(node),
        )
        .into())
    }
}

impl Attributed for SnippetConfig {
    /// The level, which [`SnippetConfig::item`] reads before the attributes.
    const LEADING: usize = 1;

    const ATTRS: Attrs<Self> = Attrs(&[
        (
            "run",
            Text,
            "The command that checks one snippet, `{file}` standing for the file it is written to and `{lang}` for its language. Without one, this build's parser for the language checks it.",
            |c, v, t, s| {
                c.run = Some(v.as_str(t, s)?);
                Ok(())
            },
        ),
        (
            "hidden",
            Text,
            "A line prefix that checks a line without showing it, for the context a fragment needs to stand on its own. Only at the very start of a line.",
            |c, v, t, s| {
                c.hidden = Some(v.as_str(t, s)?);
                Ok(())
            },
        ),
    ]);
}
