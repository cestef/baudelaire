//! `baudelaire completions`: a shell completion script on stdout.

use clap::Args;

use super::{Cli, Cx, Run};
use crate::error::Result;
use crate::error::cli::Generated;

/// Arguments for `baudelaire completions`.
#[derive(Args, Debug, Clone)]
#[command(after_help = Shell::help())]
pub struct CompletionsArgs {
    /// Shell to generate a completion script for.
    #[arg(value_enum)]
    pub shell: Shell,
}

/// A shell `completions` can generate for.
///
/// Its own enum rather than [`clap_complete::Shell`] because nushell's
/// generator ships in a separate crate and so cannot be a variant of that one.
/// This is the single table mapping the name a user types to the generator that
/// answers it, and [`Shell::script`] is the only place that mapping is written.
#[derive(clap::ValueEnum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shell {
    Bash,
    Elvish,
    Fish,
    Nushell,
    #[value(name = "powershell")]
    PowerShell,
    Zsh,
}

impl Shell {
    /// Where this shell wants the script.
    ///
    /// An exhaustive match rather than a lookup table, so adding a shell fails
    /// to compile until its install line is written; the `--help` text is
    /// rendered from these, and the value list clap prints comes from the same
    /// enum, so the two cannot drift.
    const fn install(self) -> &'static str {
        match self {
            Self::Bash => {
                "baudelaire completions bash > ~/.local/share/bash-completion/completions/baudelaire"
            }
            Self::Elvish => "baudelaire completions elvish > ~/.config/elvish/lib/baudelaire.elv",
            Self::Fish => {
                "baudelaire completions fish > ~/.config/fish/completions/baudelaire.fish"
            }
            Self::Nushell => {
                "baudelaire completions nushell > ~/.config/nushell/completions/baudelaire.nu"
            }
            Self::PowerShell => {
                "baudelaire completions powershell | Out-String | Invoke-Expression"
            }
            Self::Zsh => {
                "baudelaire completions zsh > ~/.local/share/zsh/site-functions/_baudelaire"
            }
        }
    }

    /// The per-shell install lines, appended to `completions --help`.
    ///
    /// Laid out like [`Cli::examples`], for the same reason: the shell column is
    /// measured from the rows rather than hand-tuned to the longest one.
    fn help() -> String {
        use clap::ValueEnum;
        use owo_colors::{OwoColorize, Stream::Stdout};
        use std::fmt::Write;

        let rows: Vec<_> = Self::value_variants()
            .iter()
            .filter_map(|shell| {
                let name = shell.to_possible_value()?;
                Some((name.get_name().to_owned(), shell.install()))
            })
            .collect();
        let column = rows.iter().map(|(n, _)| n.len()).max().unwrap_or(0) + 2;
        let mut out = format!(
            "{}\n",
            "Examples:".if_supports_color(Stdout, |t| t.cyan().bold().to_string())
        );
        for (name, line) in &rows {
            let pad = " ".repeat(column - name.len());
            let colored = line.if_supports_color(Stdout, |t| t.green().bold().to_string());
            let _ = writeln!(out, "  {name}{pad}{colored}");
        }
        let _ = write!(
            out,
            "\nThe directory has to exist, and the shell has to be told to read it;\n\
             {} cover both.",
            "your shell's completion docs".if_supports_color(Stdout, |t| t.dimmed().to_string())
        );
        out
    }

    /// Render the completion script for `command` under `name`.
    fn script(self, command: &mut clap::Command, name: String) -> Vec<u8> {
        use clap_complete::Shell as Builtin;

        let mut out = Vec::new();
        match self {
            Self::Nushell => {
                clap_complete::generate(clap_complete_nushell::Nushell, command, name, &mut out);
            }
            Self::Bash => clap_complete::generate(Builtin::Bash, command, name, &mut out),
            Self::Elvish => clap_complete::generate(Builtin::Elvish, command, name, &mut out),
            Self::Fish => clap_complete::generate(Builtin::Fish, command, name, &mut out),
            Self::PowerShell => {
                clap_complete::generate(Builtin::PowerShell, command, name, &mut out);
            }
            Self::Zsh => clap_complete::generate(Builtin::Zsh, command, name, &mut out),
        }
        out
    }
}

/// Both of these describe the CLI rather than a site, so they read no config,
/// touch no project, and write their one document to stdout, which `--json`
/// otherwise reserves. Nothing else is printed: the output is meant to be
/// redirected into a completion directory or `man1/`, and a banner in front of
/// it would corrupt the file.
///
/// `Command::owns_stdout` is what enforces the reservation. This paragraph used
/// to describe it and nothing implemented it, so `completions bash --json`
/// appended its summary object to the script.
impl Run for CompletionsArgs {
    fn run(&self, _cx: &Cx) -> Result<()> {
        use clap::CommandFactory;

        let mut command = Cli::command();
        let name = command.get_name().to_owned();
        // Rendered into memory first so the whole script reaches stdout in one
        // write, and so a failed write is one error rather than a half-written
        // completion file that a shell would happily source.
        let script = self.shell.script(&mut command, name);
        Generated::Completions.emit(&script)?;
        Ok(())
    }
}
