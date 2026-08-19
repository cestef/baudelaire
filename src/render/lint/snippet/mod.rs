//! Checking a code fence against the language it claims: the in-process
//! [`parser`]s, and the command a site names for a language they do not cover.

mod command;
pub mod parser;

use std::path::Path;

use crate::config::{LintConfig, SnippetConfig};
use crate::error::Lint;
use crate::render::snippet::{Fault, Snippet};

use command::Command;
use parser::{Parser, Parsers};

use super::{Check, Cx, Findings};

/// What checks the fences of one language: the command the site named for it,
/// or the parser this build carries.
enum Checker<'a> {
    Run(Command<'a>),
    Parse(&'static dyn Parser),
}

impl<'a> Checker<'a> {
    /// What `rule` asks for, or `None` for a language it turned off. A line
    /// with neither a command nor a parser is refused at config parse.
    fn of(rule: &'a SnippetConfig, lang: &str, root: &'a Path) -> Option<Self> {
        if !rule.level.on() {
            return None;
        }
        rule.run.as_deref().map_or_else(
            || Parsers::of(lang).map(Self::Parse),
            |line| Some(Self::Run(Command::new(line, root))),
        )
    }

    fn faults(&self, snippet: &Snippet) -> Vec<Fault> {
        match self {
            Self::Run(command) => command.faults(snippet),
            Self::Parse(parser) => parser.check(snippet),
        }
    }
}

/// Every fence checked as the language it claims.
pub(super) struct Snippets;

impl Check for Snippets {
    fn enabled(&self, config: &LintConfig) -> bool {
        config.snippets.iter().any(|(_, rule)| rule.level.on())
    }

    fn check(&self, _page: &super::Page, cx: &Cx<'_>, found: &mut Findings<'_>) {
        for snippet in cx.fences.iter().filter(|snippet| !snippet.ignored()) {
            let Some(checker) = cx
                .config
                .snippet(&snippet.lang)
                .and_then(|rule| Checker::of(rule, &snippet.lang, cx.root))
            else {
                continue;
            };
            for fault in checker.faults(snippet) {
                found.push(
                    snippet.at(fault.at),
                    Lint::Snippet {
                        lang: snippet.lang.clone(),
                        message: fault.message,
                    },
                );
            }
        }
    }
}
