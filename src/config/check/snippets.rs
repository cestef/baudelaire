//! `check { snippets { } }`: how the code fences of one language are checked.

use kdl::KdlNode;

use dispatch_derive::Table;

use crate::config::Level;
use crate::config::dispatch::{Attributed, Attrs};
use crate::config::node::NodeExt;
use crate::config::vocab::attr;
use crate::error::{ConfigError, ConfigErrorKind, Result};
use crate::render::lint::snippet::parser::Parsers;
use crate::ui::markup;

/// How the fences claiming one language are checked.
#[derive(Debug, Clone, Hash, Table)]
#[table(
    impl = Attributed,
    const ATTRS: Attrs<Self> = Attrs,
    rule = attr,
    items {
        /// The level, which [`SnippetConfig::item`] reads before the attributes.
        const LEADING: usize = 1;

        fn unkeyed(&self) -> Vec<crate::config::Value> {
            vec![self.level.into()]
        }
    },
)]
pub struct SnippetConfig {
    /// How loud a finding in one of them is.
    pub level: Level,
    /// The command that checks one snippet, `{file}` standing for the file it is written to and `{lang}` for its language. Without one, this build's parser for the language checks it.
    #[key(opt text)]
    pub run: Option<String>,
    /// A line prefix that checks a line without showing it, for the context a fragment needs to stand on its own. Only at the very start of a line.
    ///
    /// `None` is a language whose fences are shown entire.
    #[key(opt text)]
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
        if self.run.is_some() || !self.level.on() || Parsers::of(lang).is_some() {
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
                    Parsers::langs().join(", ")
                ),
            },
            NodeExt::span(node),
        )
        .into())
    }
}
