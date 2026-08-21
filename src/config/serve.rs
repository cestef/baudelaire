//! `serve { }`: dev server options.

use dispatch_derive::Table;

use crate::config::dispatch::Kind::Texts;
use crate::config::dispatch::{Block, Section};
use crate::config::node::NodeExt;
use crate::config::vocab::rule;
use crate::error::ConfigError;

#[derive(Debug, Clone, Hash, Table)]
pub struct ServeConfig {
    /// The port the dev server listens on.
    #[key(port)]
    pub port: u16,

    /// The address it binds. Defaults to loopback; it has no authentication.
    #[key(text)]
    pub bind: String,

    /// Open a browser when the server starts.
    #[key(flag)]
    pub open: bool,

    /// Watch the sources and rebuild. Off, it serves what is already built.
    #[key(flag)]
    pub watch: bool,

    /// Extra paths to watch, one word each.
    ///
    /// Beyond content, templates, and assets.
    #[key(texts)]
    pub include: Vec<String>,

    /// Paths to leave unwatched, one word each.
    ///
    /// Checked first, so they override both the defaults and `include`.
    #[key(texts)]
    pub exclude: Vec<String>,

    /// The command alt-clicking a preview element runs, program and arguments as separate words.
    ///
    /// `{file}`, `{line}` and `{column}` are substituted per argument, with no
    /// shell in between. Empty means no editor.
    #[key(custom(
        Texts,
        |c: &Self| c.editor.clone().into(),
        |c: &mut Self, n: &kdl::KdlNode, t: &str| {
            let span = NodeExt::span(n);
            let words = n.words(t)?;
            if let [only] = words.as_slice()
                && only.split_whitespace().count() > 1
            {
                return Err(ConfigError::command_line(t, only, span).into());
            }
            c.editor = words;
            Ok(())
        },
    ))]
    pub editor: Vec<String>,
}

impl Default for ServeConfig {
    fn default() -> Self {
        Self {
            port: 1821,
            bind: "127.0.0.1".into(),
            open: true,
            watch: true,
            include: Vec::new(),
            exclude: Vec::new(),
            editor: Vec::new(),
        }
    }
}
