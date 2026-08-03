//! The blocks appended under a command's generated help.
//!
//! Clap writes the flags; everything a reader needs *around* them is here, in
//! one rendering each. Three commands grew their own copy of the same example
//! table, so a column width and an accent colour were decided three times and
//! agreed twice.
//!
//! Colour is gated on the stdout stream itself (`if_supports_color`), so
//! escapes never leak when piped or under `NO_COLOR`: the same policy
//! [`crate::ui`] uses.

use std::fmt::Display;

use owo_colors::{OwoColorize, Stream::Stdout};

/// A `Examples:` block: one row per invocation, its description aligned past
/// the longest command.
pub(super) struct Examples<'a> {
    pub rows: &'a [(&'a str, &'a str)],
    /// A closing line under the table, for a command that has one more thing
    /// to say than its rows do.
    pub footer: Option<&'a str>,
}

impl<'a> Examples<'a> {
    pub(super) fn new(rows: &'a [(&'a str, &'a str)]) -> Self {
        Self { rows, footer: None }
    }
}

impl Display for Examples<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "{}", Heading("Examples:"))?;
        // Padding is measured on the *visible* length, so the escapes below
        // cannot skew the alignment.
        let column = self.rows.iter().map(|(c, _)| c.len()).max().unwrap_or(0) + 2;
        for (command, what) in self.rows {
            let pad = " ".repeat(column - command.len());
            writeln!(f, "  {}{pad}{what}", Literal(command))?;
        }
        match self.footer {
            Some(footer) => write!(f, "\n{footer}"),
            None => Ok(()),
        }
    }
}

/// An `About:` block: the paragraphs a command's one-line description cannot
/// carry, for a command whose *point* a reader cannot guess from its name.
pub(super) struct About<'a>(pub &'a str);

impl Display for About<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "{}", Heading("About:"))?;
        for line in self.0.lines() {
            match line.is_empty() {
                true => writeln!(f)?,
                false => writeln!(f, "  {line}")?,
            }
        }
        Ok(())
    }
}

/// A help section heading, in the structure accent.
struct Heading<'a>(&'a str);

impl Display for Heading<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let styled = self
            .0
            .if_supports_color(Stdout, |t| t.cyan().bold().to_string());
        write!(f, "{styled}")
    }
}

/// Something the reader types, in the literal accent clap gives flags and
/// commands.
pub(super) struct Literal<'a>(pub &'a str);

impl Display for Literal<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let styled = self
            .0
            .if_supports_color(Stdout, |t| t.green().bold().to_string());
        write!(f, "{styled}")
    }
}
