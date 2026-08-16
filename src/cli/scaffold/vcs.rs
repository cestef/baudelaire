//! The version control a scaffolded project starts under.

use std::path::Path;
use std::process::Command;

use owo_colors::OwoColorize;

use crate::cli::prompt::Prompt;
use crate::error::Result;
use crate::error::warning::{VcsFailed, VcsMissing, VcsUnrunnable};
use crate::ui::Ui;

/// A version-control system baudelaire can initialize for a new project. Both
/// use the same `.gitignore` (jujutsu honors it too).
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum Vcs {
    Git,
    #[value(alias = "jj")]
    Jujutsu,
}

/// What setting up one [`Vcs`] takes: the program to run and its arguments, the
/// marker directory whose presence means a repository already exists, and the
/// name to report.
pub(super) struct Tool {
    command: &'static str,
    args: &'static [&'static str],
    marker: &'static str,
    label: &'static str,
}

impl Vcs {
    /// The tool that initializes a repository. Jujutsu colocates a `.git`, so it
    /// stays interoperable with git tooling.
    const fn tool(self) -> Tool {
        match self {
            Self::Git => Tool {
                command: "git",
                args: &["init", "-q"],
                marker: ".git",
                label: "git",
            },
            Self::Jujutsu => Tool {
                command: "jj",
                args: &["git", "init", "--colocate"],
                marker: ".jj",
                label: "jujutsu",
            },
        }
    }
}

/// Optional version-control setup for a freshly scaffolded project.
pub(super) struct Repo<'a> {
    root: &'a Path,
    vcs: Vcs,
}
impl<'a> Repo<'a> {
    pub(super) fn new(root: &'a Path, vcs: Vcs) -> Self {
        Self { root, vcs }
    }

    /// Which VCS to set up, if any: an explicit `--vcs` wins, otherwise ask,
    /// and only when `interactive`, so a piped or CI run sets up nothing
    /// unbidden.
    pub(super) fn wanted(interactive: bool, explicit: Option<Vcs>) -> Result<Option<Vcs>> {
        if explicit.is_some() {
            return Ok(explicit);
        }
        if !interactive {
            return Ok(None);
        }
        Prompt::new("set up version control?")
            .option(&["git", "g", "y", "yes"], Some(Vcs::Git))
            .default()
            .option(&["jj", "jujutsu", "j"], Some(Vcs::Jujutsu))
            .option(&["no", "n"], None)
            .ask()
    }

    /// Initialize the repository, skipping the step if one already exists. A
    /// missing or failing tool is a warning, not an error: the project is
    /// scaffolded either way.
    pub(super) fn setup(&self, ui: &Ui) {
        let tool = self.vcs.tool();
        if self.root.join(tool.marker).exists() {
            return;
        }
        match Command::new(tool.command)
            .args(tool.args)
            .current_dir(self.root)
            .output()
        {
            Ok(out) if out.status.success() => {
                ui.detail(format_args!("{} {} repository", "+".green(), tool.label));
            }
            Ok(out) => {
                let detail = String::from_utf8_lossy(&out.stderr);
                let detail = detail.trim();
                ui.warn(VcsFailed {
                    tool: tool.command,
                    detail: (!detail.is_empty()).then(|| detail.to_owned()),
                });
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                ui.warn(VcsMissing { tool: tool.command });
            }
            Err(source) => ui.warn(VcsUnrunnable {
                tool: tool.command,
                source,
            }),
        }
    }
}

#[cfg(test)]
mod repo_tests {
    use super::{Repo, Vcs};

    #[test]
    fn only_an_explicit_vcs_sets_one_up_without_a_prompt() {
        assert_eq!(Repo::wanted(false, None).unwrap(), None);
        assert_eq!(Repo::wanted(false, Some(Vcs::Git)).unwrap(), Some(Vcs::Git));
        assert_eq!(
            Repo::wanted(true, Some(Vcs::Jujutsu)).unwrap(),
            Some(Vcs::Jujutsu)
        );
    }
}
