//! The blocks appended under a command's generated help: clap writes the flags,
//! and everything around them is rendered here.

use std::fmt::Display;

use owo_colors::{OwoColorize, Stream::Stdout};

/// Which column of a row names something the reader types, and so takes the
/// literal accent. The other column is prose.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Accent {
    /// `command  what it does`: the invocation leads.
    Left,
    /// `label  the line to run`: the row is keyed by something else, so the
    /// line to type is the value.
    Right,
}

/// A titled block of two aligned columns: the `Examples:` table under a
/// command, the `Exit codes:` list, the `Environment:` one.
pub(super) struct Table {
    heading: &'static str,
    rows: Vec<(String, String)>,
    accent: Accent,
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
    /// Padding is measured on the unstyled text, so the escapes cannot skew the
    /// alignment.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "{}", Heading(self.heading))?;
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
            if line.is_empty() {
                writeln!(f)?;
            } else {
                writeln!(f, "  {line}")?;
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

    /// A block as a reader without colour sees it.
    fn plain(table: &Table) -> String {
        console::strip_ansi_codes(&table.to_string()).into_owned()
    }

    #[test]
    fn a_table_aligns_its_second_column_past_the_longest_first() {
        let table = plain(&Table::examples(&[("short", "a"), ("much-longer", "b")]));
        let lines: Vec<&str> = table.lines().skip(1).collect();
        let column = |line: &str| line.rfind(' ').map(|i| i + 1);
        assert_eq!(column(lines[0]), column(lines[1]), "{table}");
    }

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

    #[test]
    fn each_block_is_titled() {
        assert!(plain(&Table::codes(&[("0", "fine")])).starts_with("Exit codes:"));
        assert!(
            plain(&Table::environment(&[("NO_COLOR", "no colour")])).starts_with("Environment:")
        );
    }

    #[test]
    fn a_footer_closes_the_block() {
        let table = Table::examples(&[("a", "b")]).footer("and one more thing");
        assert!(plain(&table).ends_with("\nand one more thing"));
    }
}
