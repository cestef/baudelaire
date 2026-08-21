//! Plumbing shared by the destinations baudelaire pushes to: the HTTP agent,
//! consent for a mutating action, and secret resolution, all behind a
//! terminal-agnostic [`Interaction`] seam.

use ureq::tls::{TlsConfig, TlsProvider};

use crate::error::{RemoteError, Result, Unattended};
use crate::ui::Ui;

/// Whether a 4xx/5xx is a failure or an answer, spelled at every call rather
/// than left to ureq's default, which is [`Fatal`](Status::Fatal).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// A non-2xx is the answer, delivered as an ordinary response.
    Read,
    /// A non-2xx is a failure, and the caller wants it as an error.
    Fatal,
}

/// The one `ureq::Agent` constructor: one TLS policy, one deadline, one user
/// agent naming the tool and what it is doing.
pub struct Http;

impl Http {
    /// Per-request ceiling, which `check { external { timeout } }` also
    /// defaults to.
    pub const TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

    /// The same for a request whose body is the point rather than its headers.
    ///
    /// A deadline here covers the transfer as well as the handshake, so an
    /// object big enough not to fit in [`TIMEOUT`](Http::TIMEOUT) could never be
    /// uploaded at all: a 30 MB export on a domestic link needs minutes.
    pub const TRANSFER: std::time::Duration = std::time::Duration::from_mins(5);

    /// An agent for the work `doing` describes, which is what an administrator
    /// reading their logs sees knocking.
    pub fn agent(doing: &str, status: Status) -> ureq::Agent {
        Self::within(doing, status, Self::TIMEOUT)
    }

    /// [`Http::agent`] for work that moves a file rather than asking about one.
    pub fn transferring(doing: &str, status: Status) -> ureq::Agent {
        Self::within(doing, status, Self::TRANSFER)
    }

    /// The same agent on a caller-chosen deadline, for the one caller a site
    /// can set one for.
    pub fn within(doing: &str, status: Status, timeout: std::time::Duration) -> ureq::Agent {
        ureq::Agent::config_builder()
            .http_status_as_error(status == Status::Fatal)
            .timeout_global(Some(timeout))
            .user_agent(format!("baudelaire/{} ({doing})", crate::VERSION))
            .tls_config(Self::tls())
            .build()
            .into()
    }

    /// The TLS configuration every agent must use.
    ///
    /// The provider is pinned because ureq defaults to rustls and this crate
    /// compiles only its `native-tls` backend, so the default panics at connect
    /// time; verification is untouched.
    fn tls() -> TlsConfig {
        TlsConfig::builder()
            .provider(TlsProvider::NativeTls)
            .build()
    }
}

/// What asking permission for a destructive action came back with; a refusal is
/// the user's answer, and [`Unattended`](Consent::Unattended) the absence of
/// one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Consent {
    Granted,
    Refused,
    /// Nobody was there to ask, and no flag settled it in advance.
    Unattended,
}

/// How a run talks to the user: confirmations and interactive secret entry.
pub trait Interaction {
    /// Whether this seam can actually put a question to someone; `false` means
    /// [`confirm`](Interaction::confirm) would answer on the user's behalf.
    fn interactive(&self) -> bool;

    /// Confirm a mutating action; `Ok(false)` cancels it. Called only when
    /// [`interactive`](Interaction::interactive) holds.
    fn confirm(&self, prompt: &str) -> Result<bool>;

    /// Prompt for a secret labeled `label`, or `Ok(None)` when the environment
    /// cannot supply one (non-interactive).
    fn secret(&self, label: &str) -> Result<Option<String>>;

    /// Whether a destructive `action` (a phrase like `deploy to s3`, or `remove
    /// every build directory`) may go ahead: `yes` grants it outright, a
    /// terminal is asked, and anything else is [`Consent::Unattended`].
    ///
    /// `action` is the phrase alone: the question mark belongs to the prompt,
    /// so the same phrase reads correctly in a diagnostic.
    fn consent(&self, action: &str, yes: bool) -> Result<Consent> {
        if yes {
            return Ok(Consent::Granted);
        }
        if !self.interactive() {
            return Ok(Consent::Unattended);
        }
        Ok(if self.confirm(&format!("{action}?"))? {
            Consent::Granted
        } else {
            Consent::Refused
        })
    }
}

/// Cross-cutting options for a push, backend-neutral.
pub struct Options<'a> {
    /// Report what would change without writing to any destination.
    pub dry_run: bool,
    /// Skip the confirmation prompt.
    pub yes: bool,
    /// A secret supplied on the command line, preferred over the environment
    /// variable and the interactive prompt.
    pub secret: Option<String>,
    pub interaction: &'a dyn Interaction,
}

impl Options<'_> {
    /// Run every configured backend over `payload` in turn, confirming before
    /// each one writes anything. `verb` names the action in the prompt,
    /// `summary` the size of the payload in the section header.
    pub fn publish<P>(
        &self,
        verb: &str,
        backends: Vec<Box<dyn Backend<P>>>,
        payload: &P,
        summary: impl Fn(&P) -> String,
        ui: &Ui,
    ) -> Result<()> {
        for backend in backends {
            ui.section(format_args!("{} - {}", backend.name(), summary(payload)));
            if !self.dry_run && !self.confirm(&format!("{verb} to {}", backend.name()))? {
                ui.detail(format_args!("skipped {}", backend.name()));
                continue;
            }
            backend.run(payload, self, ui)?;
        }
        Ok(())
    }

    /// Resolve a secret: the CLI value (or stdin when it is the conventional
    /// `-`), else the `env` variable, else an interactive prompt labeled
    /// `label`. An empty value from any source is no secret, never an empty
    /// password.
    pub fn secret(&self, env: &str, label: &str) -> Result<String> {
        if let Some(secret) = &self.secret {
            if secret != "-" {
                return Ok(secret.clone());
            }
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
        self.interaction
            .secret(label)?
            .ok_or_else(|| Self::missing(label))
    }

    /// Consent to a mutating `action`, resolved by [`Interaction::consent`], an
    /// unattended run being an error rather than a skip.
    pub fn confirm(&self, action: &str) -> Result<bool> {
        match self.interaction.consent(action, self.yes)? {
            Consent::Granted => Ok(true),
            Consent::Refused => Ok(false),
            Consent::Unattended => Err(Unattended {
                action: action.to_owned(),
            }
            .into()),
        }
    }

    fn missing(label: &str) -> crate::error::BaudelaireErrorKind {
        RemoteError::MissingSecret {
            label: label.to_owned(),
        }
        .into()
    }

    /// Read one line from stdin as a secret, stripping the trailing newline and
    /// taking the rest verbatim.
    fn stdin_line() -> Result<String> {
        use std::io::BufRead;
        let mut line = String::new();
        std::io::stdin().lock().read_line(&mut line)?;
        Ok(line.trim_end_matches(['\r', '\n']).to_owned())
    }
}

/// One publishing destination for a payload of type `P` (the built files for a
/// deploy, the publishable documents for an announce).
pub trait Backend<P> {
    /// Stable, human-facing name, shown in progress output.
    fn name(&self) -> &'static str;

    /// Publish `payload` under `opts`, reporting the plan without writing under
    /// `opts.dry_run`.
    fn run(&self, payload: &P, opts: &Options, ui: &Ui) -> Result<()>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::{BaudelaireErrorKind, RemoteError};

    #[test]
    fn consent_separates_a_refusal_from_nobody_being_there() {
        let attended = Stub {
            confirm: false,
            ..Stub::default()
        };
        assert_eq!(attended.consent("wipe", false).unwrap(), Consent::Refused);
        assert_eq!(attended.consent("wipe", true).unwrap(), Consent::Granted);
        let unattended = Stub {
            interactive: false,
            ..Stub::default()
        };
        assert_eq!(
            unattended.consent("wipe", false).unwrap(),
            Consent::Unattended
        );
    }

    /// A headless [`Interaction`]: a fixed confirmation answer and an optional
    /// prompt secret.
    struct Stub {
        confirm: bool,
        secret: Option<String>,
        interactive: bool,
    }

    impl Default for Stub {
        fn default() -> Self {
            Self {
                confirm: true,
                secret: None,
                interactive: true,
            }
        }
    }

    impl Interaction for Stub {
        fn interactive(&self) -> bool {
            self.interactive
        }
        fn confirm(&self, _prompt: &str) -> Result<bool> {
            Ok(self.confirm)
        }
        fn secret(&self, _label: &str) -> Result<Option<String>> {
            Ok(self.secret.clone())
        }
    }

    fn options(secret: Option<String>, stub: &Stub) -> Options<'_> {
        Options {
            dry_run: false,
            yes: false,
            secret,
            interaction: stub,
        }
    }

    /// An env var no test sets, so secret resolution falls past the env step.
    const UNSET: &str = "BAUDELAIRE_TEST_UNSET_SECRET";

    #[test]
    fn secret_prefers_the_cli_value() {
        let stub = Stub {
            secret: Some("prompted".into()),
            ..Stub::default()
        };
        let opts = options(Some("flag".into()), &stub);
        assert_eq!(opts.secret(UNSET, "pw").unwrap(), "flag");
    }

    #[test]
    fn secret_falls_back_to_the_prompt() {
        let stub = Stub {
            secret: Some("prompted".into()),
            ..Stub::default()
        };
        let opts = options(None, &stub);
        assert_eq!(opts.secret(UNSET, "pw").unwrap(), "prompted");
    }

    #[test]
    fn secret_missing_when_no_source_can_supply_it() {
        let stub = Stub::default();
        let opts = options(None, &stub);
        assert!(matches!(
            opts.secret(UNSET, "pw"),
            Err(BaudelaireErrorKind::Remote(
                RemoteError::MissingSecret { .. }
            ))
        ));
    }

    #[test]
    fn confirm_short_circuits_under_yes() {
        let stub = Stub {
            confirm: false,
            ..Stub::default()
        };
        let opts = Options {
            yes: true,
            ..options(None, &stub)
        };
        assert!(opts.confirm("deploy to s3").unwrap());
    }

    #[test]
    fn confirm_refuses_rather_than_assuming_an_answer_off_a_terminal() {
        let stub = Stub {
            interactive: false,
            ..Stub::default()
        };
        let opts = options(None, &stub);
        assert!(matches!(
            opts.confirm("deploy to s3"),
            Err(BaudelaireErrorKind::Unattended(_))
        ));
    }

    #[test]
    fn yes_still_carries_a_non_interactive_run() {
        let stub = Stub {
            confirm: false,
            interactive: false,
            ..Stub::default()
        };
        let opts = Options {
            yes: true,
            ..options(None, &stub)
        };
        assert!(opts.confirm("deploy to s3").unwrap());
    }
}
