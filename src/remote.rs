//! Plumbing shared by the destinations baudelaire pushes to — [`crate::announce`]
//! (metadata records) and [`crate::deploy`] (files). Both confirm mutating
//! actions, honor `--dry-run`/`--yes`, and resolve a secret the same way, so
//! that lives here once behind a terminal-agnostic [`Interaction`] seam.

use crate::error::{RemoteError, Result};

/// How a run talks to the user: confirmations and interactive secret entry. The
/// remote layers depend only on this, never on the terminal, so the CLI backs it
/// with the shared prompt widgets and tests pass a headless stub.
pub trait Interaction {
    /// Confirm a mutating action; `Ok(false)` cancels it.
    fn confirm(&self, prompt: &str) -> Result<bool>;

    /// Prompt for a secret labeled `label`, or `Ok(None)` when the environment
    /// cannot supply one (non-interactive).
    fn secret(&self, label: &str) -> Result<Option<String>>;
}

/// Cross-cutting options for a push, backend-neutral. A backend reads `dry_run`
/// to preview without writing and resolves its own secret through [`Options::secret`];
/// confirmation runs generically before a backend does.
pub struct Options<'a> {
    /// Report what would change without writing to any destination.
    pub dry_run: bool,
    /// Skip the confirmation prompt.
    pub yes: bool,
    /// A secret supplied on the command line — preferred over the environment
    /// variable and the interactive prompt.
    pub secret: Option<String>,
    /// The user-interaction backend (terminal in the CLI, a stub in tests).
    pub interaction: &'a dyn Interaction,
}

impl Options<'_> {
    /// Resolve a secret: the CLI value (or stdin when it is the conventional
    /// `-`), else the `env` variable, else an interactive prompt labeled `label`.
    /// The one place credential acquisition lives, shared by every backend.
    pub fn secret(&self, env: &str, label: &str) -> Result<String> {
        if let Some(secret) = &self.secret {
            if secret != "-" {
                return Ok(secret.clone());
            }
            // A closed or blank stdin is "no secret", not an empty password —
            // matches the env and prompt branches, which both reject empty.
            let line = Self::stdin_line()?;
            if line.is_empty() {
                return Err(Self::missing(label));
            }
            return Ok(line);
        }
        if let Ok(secret) = std::env::var(env)
            && !secret.is_empty()
        {
            return Ok(secret);
        }
        self.interaction.secret(label)?.ok_or_else(|| Self::missing(label))
    }

    /// Confirm a mutating action, short-circuiting to `true` under `--yes`.
    pub fn confirm(&self, prompt: &str) -> Result<bool> {
        if self.yes {
            return Ok(true);
        }
        self.interaction.confirm(prompt)
    }

    /// The "no secret could be found" error for `label`.
    fn missing(label: &str) -> crate::error::BaudelaireErrorKind {
        RemoteError::MissingSecret { label: label.to_owned() }.into()
    }

    /// Read one line from stdin as a secret — the conventional `-` value for a
    /// secret flag, for piping without exposing it in argv. The trailing newline
    /// is stripped; the rest is taken verbatim.
    fn stdin_line() -> Result<String> {
        use std::io::BufRead;
        let mut line = String::new();
        std::io::stdin().lock().read_line(&mut line)?;
        Ok(line.trim_end_matches(['\r', '\n']).to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::{BaudelaireErrorKind, RemoteError};

    /// A headless [`Interaction`]: a fixed confirmation answer and an optional
    /// prompt secret.
    struct Stub {
        confirm: bool,
        secret: Option<String>,
    }

    impl Interaction for Stub {
        fn confirm(&self, _prompt: &str) -> Result<bool> {
            Ok(self.confirm)
        }
        fn secret(&self, _label: &str) -> Result<Option<String>> {
            Ok(self.secret.clone())
        }
    }

    fn options<'a>(secret: Option<String>, stub: &'a Stub) -> Options<'a> {
        Options { dry_run: false, yes: false, secret, interaction: stub }
    }

    /// An env var no test sets, so secret resolution falls past the env step.
    const UNSET: &str = "BAUDELAIRE_TEST_UNSET_SECRET";

    #[test]
    fn secret_prefers_the_cli_value() {
        let stub = Stub { confirm: true, secret: Some("prompted".into()) };
        let opts = options(Some("flag".into()), &stub);
        assert_eq!(opts.secret(UNSET, "pw").unwrap(), "flag");
    }

    #[test]
    fn secret_falls_back_to_the_prompt() {
        let stub = Stub { confirm: true, secret: Some("prompted".into()) };
        let opts = options(None, &stub);
        assert_eq!(opts.secret(UNSET, "pw").unwrap(), "prompted");
    }

    #[test]
    fn secret_missing_when_no_source_can_supply_it() {
        let stub = Stub { confirm: true, secret: None };
        let opts = options(None, &stub);
        assert!(matches!(
            opts.secret(UNSET, "pw"),
            Err(BaudelaireErrorKind::Remote(RemoteError::MissingSecret { .. }))
        ));
    }

    #[test]
    fn confirm_short_circuits_under_yes() {
        let stub = Stub { confirm: false, secret: None };
        let opts = Options { yes: true, ..options(None, &stub) };
        assert!(opts.confirm("go?").unwrap());
    }
}
