//! `baudelaire completions`: a shell completion script on stdout.

use clap::Args;

use super::{Cli, Cx, Run, help};
use crate::error::Result;
use crate::error::cli::Generated;

#[derive(Args, Debug, Clone)]
#[command(after_help = Shell::help())]
pub struct CompletionsArgs {
    /// Shell to generate a completion script for.
    #[arg(value_enum)]
    pub shell: Shell,
}

/// A shell `completions` can generate for; its own enum rather than
/// [`clap_complete::Shell`] because nushell's generator ships in a separate
/// crate and so cannot be a variant of that one.
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
    /// Where this shell wants the script; an exhaustive match, so adding a
    /// shell fails to compile until its install line is written.
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
    fn help() -> String {
        use clap::ValueEnum;
        use owo_colors::{OwoColorize, Stream::Stdout};

        let rows = Self::value_variants().iter().filter_map(|shell| {
            let name = shell.to_possible_value()?;
            Some((name.get_name().to_owned(), shell.install().to_owned()))
        });
        help::Table::keyed(rows)
            .footer(format!(
                "The directory has to exist, and the shell has to be told to read it;\n\
                 {} cover both.",
                "your shell's completion docs"
                    .if_supports_color(Stdout, |t| t.dimmed().to_string())
            ))
            .to_string()
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

/// Writes its one document to stdout and nothing else: the output is meant to
/// be redirected into a completion directory, and a banner would corrupt it.
impl Run for CompletionsArgs {
    fn run(&self, _cx: &Cx) -> Result<()> {
        use clap::CommandFactory;

        let mut command = Cli::command();
        let name = command.get_name().to_owned();
        let script = self.shell.script(&mut command, name);
        Generated::Completions.emit(&script)?;
        Ok(())
    }
}
