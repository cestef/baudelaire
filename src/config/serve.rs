//! `serve { }`: dev server options.

use crate::config::dispatch::Kind::{Flag, Number, Text, Texts};
use crate::config::dispatch::{Block, Section};
use crate::config::node::NodeExt;
use crate::error::ConfigError;

/// Dev server options.
#[derive(Debug, Clone, Hash)]
pub struct ServeConfig {
    /// Port to listen on.
    pub port: u16,
    /// Address to bind.
    pub bind: String,
    /// Open browser on start.
    pub open: bool,
    /// Watch for changes and rebuild.
    pub watch: bool,
    /// Extra paths to watch, beyond content, templates, and assets (e.g. a data
    /// directory or a Tailwind input outside `assets/`).
    pub include: Vec<String>,
    /// Paths the watcher ignores (e.g. hook-generated files), so a `before`
    /// hook writing into a watched directory does not trigger a rebuild loop.
    /// Checked first, so it overrides both the defaults and `include`.
    pub exclude: Vec<String>,
    /// The command that opens a source location, run when a preview alt-click
    /// asks for one. The program first, then each argument as its own word,
    /// with `{file}`, `{line}` and `{column}` substituted per argument: no
    /// shell, so a path is never re-parsed as a command line.
    ///
    /// Empty means no editor, and the preview says so rather than guessing at
    /// one.
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
            // No guess: launching a program is the author's instruction, and
            // `$EDITOR` is as likely to be a terminal editor with no terminal
            // to appear in.
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
            |c, n, t| {
                c.port = n.port(t, 0)?;
                Ok(())
            },
        ),
        (
            "bind",
            Text,
            "The address it binds. Defaults to loopback; it has no authentication.",
            |c, n, t| {
                c.bind = n.string(t, 0)?;
                Ok(())
            },
        ),
        (
            "open",
            Flag,
            "Open a browser when the server starts.",
            |c, n, t| {
                c.open = n.boolean(t, 0)?;
                Ok(())
            },
        ),
        (
            "watch",
            Flag,
            "Watch the sources and rebuild. Off, it serves what is already built.",
            |c, n, t| {
                c.watch = n.boolean(t, 0)?;
                Ok(())
            },
        ),
        (
            "include",
            Texts,
            "Extra paths to watch, one word each.",
            |c, n, t| {
                c.include = n.words(t)?;
                Ok(())
            },
        ),
        (
            "exclude",
            Texts,
            "Paths to leave unwatched, one word each.",
            |c, n, t| {
                c.exclude = n.words(t)?;
                Ok(())
            },
        ),
        // The program and its arguments, each its own word: the command is run
        // directly, never through a shell, so a whole command line in one
        // string would name a program that does not exist. Caught here, where
        // the span points at what the author wrote, rather than as a spawn
        // failure on the first alt-click.
        (
            "editor",
            Texts,
            "The command alt-clicking a preview element runs, program and arguments as separate words.",
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
