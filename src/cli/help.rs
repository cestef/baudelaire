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

/// Which column of a row names something the reader types, and so takes the
/// literal accent. The other column is prose.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Accent {
    /// `command  what it does`: the invocation leads.
    Left,
    /// `label  the line to run`: the row is keyed by something else, and the
    /// line to type is the value. `completions` keys its install lines by shell.
    Right,
}

/// A titled block of two aligned columns: the `Examples:` table under a
/// command, the `Exit codes:` list, the `Environment:` one.
///
/// One renderer for all of them, so the column width is measured from the rows
/// rather than hand-tuned to the longest, and the heading and the literal
/// accent are decided once. A new section is a heading const and a constructor,
/// not a second layout.
pub(super) struct Table {
    heading: &'static str,
    /// `(left, right)`. The left column is padded so the right lines up.
    rows: Vec<(String, String)>,
    accent: Accent,
    /// A closing line under the table, for a block that has one more thing to
    /// say than its rows do.
    footer: Option<String>,
}

impl Table {
    /// The headings, spelled once each.
    const EXAMPLES: &'static str = "Examples:";
    const CODES: &'static str = "Exit codes:";
    const ENVIRONMENT: &'static str = "Environment:";

    fn new(heading: &'static str, accent: Accent) -> Self {
        Self {
            heading,
            rows: Vec::new(),
            accent,
            footer: None,
        }
    }

    /// The `Examples:` block: one row per invocation, and what it does.
    pub(super) fn examples(rows: &[(&str, &str)]) -> Self {
        Self::new(Self::EXAMPLES, Accent::Left).with(rows)
    }

    /// An `Examples:` block keyed by something other than the command, so the
    /// line to type is the right-hand column.
    pub(super) fn keyed(rows: impl IntoIterator<Item = (String, String)>) -> Self {
        let mut table = Self::new(Self::EXAMPLES, Accent::Right);
        table.rows = rows.into_iter().collect();
        table
    }

    /// The `Exit codes:` block: what the process exits with, and when.
    pub(super) fn codes(rows: &[(&str, &str)]) -> Self {
        Self::new(Self::CODES, Accent::Left).with(rows)
    }

    /// The `Environment:` block: the variables a run reads.
    pub(super) fn environment(rows: &[(&str, &str)]) -> Self {
        Self::new(Self::ENVIRONMENT, Accent::Left).with(rows)
    }

    fn with(mut self, rows: &[(&str, &str)]) -> Self {
        self.rows = rows
            .iter()
            .map(|(left, right)| ((*left).to_owned(), (*right).to_owned()))
            .collect();
        self
    }

    pub(super) fn footer(mut self, footer: impl Into<String>) -> Self {
        self.footer = Some(footer.into());
        self
    }
}

impl Display for Table {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "{}", Heading(self.heading))?;
        // Padding is measured on the *visible* length, so the escapes below
        // cannot skew the alignment.
        let column = self.rows.iter().map(|(l, _)| l.len()).max().unwrap_or(0) + 2;
        for (left, right) in &self.rows {
            let pad = " ".repeat(column - left.len());
            match self.accent {
                Accent::Left => writeln!(f, "  {}{pad}{right}", Literal(left))?,
                Accent::Right => writeln!(f, "  {left}{pad}{}", Literal(right))?,
            }
        }
        match &self.footer {
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

#[cfg(test)]
mod tests {
    use super::{Accent, Table};

    /// A block as a reader without colour sees it. Stripped rather than assumed
    /// plain: whether these render styled is the stream's business (and, since
    /// `--color`, the flag's), and neither is this test's.
    fn plain(table: &Table) -> String {
        console::strip_ansi_codes(&table.to_string()).into_owned()
    }

    /// The description column is computed from the rows, not hand-tuned: the
    /// longest command decides where every description starts.
    #[test]
    fn a_table_aligns_its_second_column_past_the_longest_first() {
        let table = plain(&Table::examples(&[("short", "a"), ("much-longer", "b")]));
        let lines: Vec<&str> = table.lines().skip(1).collect();
        let column = |line: &str| line.rfind(' ').map(|i| i + 1);
        assert_eq!(column(lines[0]), column(lines[1]), "{table}");
    }

    /// A block keyed by something else puts the line to type on the right, and
    /// still lines the two columns up.
    #[test]
    fn a_keyed_table_pads_the_key_column() {
        let table = Table::keyed([
            ("bash".to_owned(), "run this".to_owned()),
            ("powershell".to_owned(), "run that".to_owned()),
        ]);
        assert_eq!(table.accent, Accent::Right);
        let rendered = plain(&table);
        assert!(rendered.starts_with("Examples:\n"), "{rendered}");
        assert!(rendered.contains("  bash        run this\n"), "{rendered}");
    }

    /// Every block writes its own heading, so a new section cannot ship under
    /// the wrong one.
    #[test]
    fn each_block_is_titled() {
        assert!(plain(&Table::codes(&[("0", "fine")])).starts_with("Exit codes:"));
        assert!(
            plain(&Table::environment(&[("NO_COLOR", "no colour")])).starts_with("Environment:")
        );
    }

    /// A footer hangs one blank line under the last row.
    #[test]
    fn a_footer_closes_the_block() {
        let table = Table::examples(&[("a", "b")]).footer("and one more thing");
        assert!(plain(&table).ends_with("\nand one more thing"));
    }
}
