//! Plumbing shared by the destinations baudelaire pushes to: [`crate::announce`]
//! (metadata records) and [`crate::deploy`] (files). Both confirm mutating
//! actions, honor `--dry-run`/`--yes`, and resolve a secret the same way, so
//! that lives here once behind a terminal-agnostic [`Interaction`] seam.

use ureq::tls::{TlsConfig, TlsProvider};

use crate::error::{RemoteError, Result, Unattended};
use crate::ui::Ui;

/// Whether a 4xx/5xx is a failure or an answer.
///
/// ureq surfaces a non-2xx as a transport error by default, which is right for
/// a caller with nothing to say about the body and wrong for one whose whole
/// job is to read it. Spelled as a choice at each call rather than left to the
/// default, because the default is the surprising one: S3's agent took it, so
/// the branch that reads the bucket's own `<Error><Code>` body was unreachable
/// and every refused request reported a bare status instead of the reason.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// A non-2xx is the answer, delivered as an ordinary response.
    Read,
    /// A non-2xx is a failure, and the caller wants it as an error.
    Fatal,
}

/// The one `ureq::Agent` constructor: one TLS policy, one deadline, one user
/// agent naming the tool and what it is doing.
///
/// It was three builders with three policies, and the drift went the wrong way
/// round: the link checker, which only reads other people's sites, had a
/// deadline, while deploy and announce, which push credentials and mutate
/// remote state, had none at all. A PUT to a black-holed endpoint blocked the
/// process for ever with nothing to cancel it.
pub struct Http;

impl Http {
    /// Per-request ceiling. Generous enough for a slow host, short enough that
    /// one black hole does not hold up the run.
    const TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

    /// An agent for the work `doing` describes, which is what an administrator
    /// reading their logs sees knocking.
    pub fn agent(doing: &str, status: Status) -> ureq::Agent {
        ureq::Agent::config_builder()
            .http_status_as_error(status == Status::Fatal)
            .timeout_global(Some(Self::TIMEOUT))
            .user_agent(format!("baudelaire/{} ({doing})", crate::VERSION))
            .tls_config(Self::tls())
            .build()
            .into()
    }

    /// The TLS configuration every agent must use.
    ///
    /// ureq 3 defaults its provider to rustls, but we compile it with only the
    /// `native-tls` backend (so the musl release can statically link a vendored
    /// OpenSSL). Left at the default, any `https://` request panics at connect
    /// time with "provider is Rustls but feature is not enabled". Pinning the
    /// provider to native-tls is what makes HTTPS actually work; verification
    /// stays on (ureq's default), so this only selects the backend, it does not
    /// weaken trust.
    fn tls() -> TlsConfig {
        TlsConfig::builder()
            .provider(TlsProvider::NativeTls)
            .build()
    }
}

/// What asking permission for a destructive action came back with.
///
/// [`Unattended`](Consent::Unattended) is separate from a plain refusal because
/// the two want different treatment and one typed error cannot serve both: a
/// refusal is the user's answer, an unattended run is the absence of one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Consent {
    Granted,
    Refused,
    /// Nobody was there to ask, and no flag settled it in advance.
    Unattended,
}

/// How a run talks to the user: confirmations and interactive secret entry. The
/// remote layers depend only on this, never on the terminal, so the CLI backs it
/// with the shared prompt widgets and tests pass a headless stub.
pub trait Interaction {
    /// Whether this seam can actually put a question to someone.
    ///
    /// `false` means [`confirm`](Interaction::confirm) would answer on the
    /// user's behalf rather than ask, so a caller needing real consent has to
    /// fail instead of accepting an answer it invented.
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
    /// THE consent policy, so nothing re-derives it. Distinguishing the third
    /// case is the point: the prompt answers with its default off a terminal,
    /// and that default is "no", so a CI run that forgot `--yes` used to skip
    /// every deploy backend and exit 0 having published nothing.
    fn consent(&self, action: &str, yes: bool) -> Result<Consent> {
        if yes {
            return Ok(Consent::Granted);
        }
        if !self.interactive() {
            return Ok(Consent::Unattended);
        }
        // The `?` is the prompt's, added here: what callers pass is the action
        // itself, so it reads correctly in a diagnostic too.
        Ok(match self.confirm(&format!("{action}?"))? {
            true => Consent::Granted,
            false => Consent::Refused,
        })
    }
}

/// Cross-cutting options for a push, backend-neutral. A backend reads `dry_run`
/// to preview without writing and resolves its own secret through [`Options::secret`];
/// confirmation runs generically before a backend does.
pub struct Options<'a> {
    /// Report what would change without writing to any destination.
    pub dry_run: bool,
    /// Skip the confirmation prompt.
    pub yes: bool,
    /// A secret supplied on the command line, preferred over the environment
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
            // A closed or blank stdin is "no secret", not an empty password:
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
        self.interaction
            .secret(label)?
            .ok_or_else(|| Self::missing(label))
    }

    /// Consent to a mutating `action`, resolved by [`Interaction::consent`].
    /// An unattended run is an error rather than a skip: a publish nobody
    /// authorized must not report success.
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

    /// The "no secret could be found" error for `label`.
    fn missing(label: &str) -> crate::error::BaudelaireErrorKind {
        RemoteError::MissingSecret {
            label: label.to_owned(),
        }
        .into()
    }

    /// Read one line from stdin as a secret: the conventional `-` value for a
    /// secret flag, for piping without exposing it in argv. The trailing newline
    /// is stripped; the rest is taken verbatim.
    fn stdin_line() -> Result<String> {
        use std::io::BufRead;
        let mut line = String::new();
        std::io::stdin().lock().read_line(&mut line)?;
        Ok(line.trim_end_matches(['\r', '\n']).to_owned())
    }
}

/// One publishing destination for a payload of type `P` (the built files for a
/// deploy, the publishable documents for an announce).
///
/// Generic over the payload because `announce` and `deploy` had a trait, a
/// registry and a run loop each, identical but for what they carried.
pub trait Backend<P> {
    /// Stable, human-facing name, shown in progress output.
    fn name(&self) -> &'static str;

    /// Publish `payload` under `opts`, reporting progress. Honors
    /// `opts.dry_run` by computing and reporting the plan without writing.
    fn run(&self, payload: &P, opts: &Options, ui: &Ui) -> Result<()>;
}

/// Run every configured backend over `payload` in turn, confirming before each
/// one writes anything. `verb` names the action in the prompt, `summary` the
/// size of the payload in the section header.
pub fn publish<P>(
    verb: &str,
    backends: Vec<Box<dyn Backend<P>>>,
    payload: &P,
    summary: impl Fn(&P) -> String,
    opts: &Options,
    ui: &Ui,
) -> Result<()> {
    for backend in backends {
        ui.section(format_args!("{} - {}", backend.name(), summary(payload)));
        // Consent before any network mutation, unless previewing.
        if !opts.dry_run && !opts.confirm(&format!("{verb} to {}", backend.name()))? {
            ui.detail(format_args!("skipped {}", backend.name()));
            continue;
        }
        backend.run(payload, opts, ui)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::{BaudelaireErrorKind, RemoteError};

    /// The consent policy is shared, so `clean` gets the same three answers to
    /// the same question, and the tests below exercise it through `Options`.
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
    /// prompt secret. `interactive` stands in for having a terminal, so the
    /// refusal path can be exercised without one.
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

    /// The CI shape: no terminal, no `--yes`. This used to take the prompt's
    /// default, which is `no`, so every backend was skipped and the run exited
    /// 0 having published nothing.
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

    /// ...and `--yes` is still the way through, terminal or not: the refusal
    /// must not have made non-interactive use impossible, only explicit.
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
