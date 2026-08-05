//! The version control a scaffolded project starts under.

use std::path::Path;
use std::process::Command;

use owo_colors::OwoColorize;

use crate::cli::prompt::Prompt;
use crate::error::Result;
use crate::error::warning::{VcsFailed, VcsMissing};
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
/// name to report. One row per variant, so a new VCS is one row and nothing
/// about it can be declared in one match and forgotten in another.
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

/// Optional version-control setup for a freshly scaffolded project. Opt-in,
/// because not every scaffold wants a repository; the `.gitignore` both systems
/// read is written unconditionally by [`Scaffold::ignore`], since it describes
/// the build output rather than the repository.
pub(super) struct Repo<'a> {
    root: &'a Path,
    vcs: Vcs,
}
impl<'a> Repo<'a> {
    pub(super) fn new(root: &'a Path, vcs: Vcs) -> Self {
        Self { root, vcs }
    }

    /// Which VCS to set up, if any. An explicit `--vcs` wins; otherwise ask,
    /// but only when the session is `interactive` (decided once, in `init`);
    /// piped or CI input sets up nothing, so a scaffold never blocks nor
    /// creates a repo unbidden.
    ///
    /// `--yes` means only "do not prompt". It used to also create a git
    /// repository, so the one flag every script reaches for to silence the
    /// prompts wrote a repository nobody asked for, and its help line ("skip the
    /// prompt and set up version control") was doing two jobs at once. Naming
    /// `--vcs` is now the only way to ask.
    pub(super) fn wanted(interactive: bool, explicit: Option<Vcs>) -> Result<Option<Vcs>> {
        if explicit.is_some() {
            return Ok(explicit);
        }
        if !interactive {
            return Ok(None);
        }
        // git is the default (empty answer); every option lists its aliases.
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
        // Capture the tool's output rather than inherit it: jj in particular
        // prints an "Initialized repo" line and a hint that would clutter the
        // scaffold log. Surface it only if the command actually failed.
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
            Err(_) => ui.warn(VcsMissing { tool: tool.command }),
        }
    }
}

#[cfg(test)]
mod repo_tests {
    use super::{Repo, Vcs};

    /// A non-interactive scaffold sets up nothing unless `--vcs` names it.
    /// `--yes` used to mean git as well as "do not prompt", so the flag scripts
    /// reach for to silence the prompts left a repository behind.
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
