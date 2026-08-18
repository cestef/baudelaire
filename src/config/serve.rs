//! `serve { }`: dev server options.

use crate::config::dispatch::Kind::{Flag, Number, Text, Texts};
use crate::config::dispatch::{Block, Section};
use crate::config::node::NodeExt;
use crate::error::ConfigError;

#[derive(Debug, Clone, Hash)]
pub struct ServeConfig {
    pub port: u16,
    pub bind: String,
    pub open: bool,
    pub watch: bool,
    /// Extra paths to watch, beyond content, templates, and assets.
    pub include: Vec<String>,
    /// Paths the watcher ignores, checked first so they override both the
    /// defaults and `include`.
    pub exclude: Vec<String>,
    /// The command a preview alt-click runs to open a source location: the
    /// program, then each argument as its own word, with `{file}`, `{line}` and
    /// `{column}` substituted per argument and no shell in between. Empty means
    /// no editor.
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

impl Section for ServeConfig {
    const RULES: Block<Self> = Block(&[
        (
            "port",
            Number,
            "The port the dev server listens on.",
            |c| c.port.into(),
            |c, n, t| {
                c.port = n.port(t, 0)?;
                Ok(())
            },
        ),
        (
            "bind",
            Text,
            "The address it binds. Defaults to loopback; it has no authentication.",
            |c| c.bind.clone().into(),
            |c, n, t| {
                c.bind = n.string(t, 0)?;
                Ok(())
            },
        ),
        (
            "open",
            Flag,
            "Open a browser when the server starts.",
            |c| c.open.into(),
            |c, n, t| {
                c.open = n.boolean(t, 0)?;
                Ok(())
            },
        ),
        (
            "watch",
            Flag,
            "Watch the sources and rebuild. Off, it serves what is already built.",
            |c| c.watch.into(),
            |c, n, t| {
                c.watch = n.boolean(t, 0)?;
                Ok(())
            },
        ),
        (
            "include",
            Texts,
            "Extra paths to watch, one word each.",
            |c| c.include.clone().into(),
            |c, n, t| {
                c.include = n.words(t)?;
                Ok(())
            },
        ),
        (
            "exclude",
            Texts,
            "Paths to leave unwatched, one word each.",
            |c| c.exclude.clone().into(),
            |c, n, t| {
                c.exclude = n.words(t)?;
                Ok(())
            },
        ),
        (
            "editor",
            Texts,
            "The command alt-clicking a preview element runs, program and arguments as separate words.",
            |c| c.editor.clone().into(),
            |c, n, t| {
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
        ),
    ]);
}
