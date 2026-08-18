//! Errors from the commands that describe the CLI itself rather than build a
//! site: `completions` and `man`.

use miette::Diagnostic;
use thiserror::Error;

use crate::ui::Code;

/// What was being written when a generated document failed to reach stdout.
#[derive(Debug, Clone, Copy)]
pub enum Generated {
    Completions,
    Man,
    Reference,
    /// What `config get` answers with, which a script reads off stdout.
    Value,
}

impl Generated {
    const fn command(self) -> &'static str {
        match self {
            Self::Completions => "baudelaire completions",
            Self::Man => "baudelaire man",
            Self::Reference => "baudelaire reference",
            Self::Value => "baudelaire config get",
        }
    }

    const fn what(self) -> &'static str {
        match self {
            Self::Completions => "completion script",
            Self::Man => "man page",
            Self::Reference => "config reference",
            Self::Value => "value",
        }
    }

    /// Write this document to stdout and flush it: stdout is block-buffered
    /// when redirected, so a write that only reaches the buffer loses its tail
    /// if the implicit flush at exit fails.
    pub fn emit(self, bytes: &[u8]) -> Result<(), WriteFailed> {
        use std::io::Write;

        let mut out = std::io::stdout().lock();
        self.check(out.write_all(bytes).and_then(|()| out.flush()))
    }

    /// Fold a write result into an error, a broken pipe being the reader going
    /// away rather than a failure of this command.
    pub fn check(self, result: std::io::Result<()>) -> Result<(), WriteFailed> {
        match result {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::BrokenPipe => Ok(()),
            Err(source) => Err(WriteFailed {
                generated: self,
                source,
            }),
        }
    }
}

/// A generated document could not be written to stdout, a broken pipe never
/// reaching here.
#[derive(Debug, Error, Diagnostic)]
#[error("failed to write the {} to stdout", .generated.what())]
#[diagnostic(
    code(baudelaire::cli::write),
    help("{} writes to stdout; redirect it to a file you can write", Code(.generated.command()))
)]
pub struct WriteFailed {
    pub generated: Generated,
    #[source]
    pub source: std::io::Error,
}

#[derive(Debug, Error, Diagnostic)]
#[error("no config key at {}", Code(.key))]
#[diagnostic(code(baudelaire::cli::unknown_key), help("{help}"))]
pub struct UnknownKey {
    pub key: String,
    pub help: String,
}

impl UnknownKey {
    /// The error for `key`, its help pointing at the nearest key there is.
    pub fn at(key: &str) -> Self {
        use crate::config::dispatch::Keys;
        use crate::config::reference::Reference;
        use crate::ui::markup;

        let all = Reference::new();
        let paths = all.paths();
        Self {
            key: key.to_owned(),
            help: Keys::of(&paths).nearest(key).map_or_else(
                || markup!("run `{}` for every key", "baudelaire reference"),
                |near| markup!("did you mean `{}`?", near),
            ),
        }
    }
}

/// A key the config never sets, which `get` and `show` have no answer for.
#[derive(Debug, Error, Diagnostic)]
#[error("this config does not set {}", Code(.key))]
#[diagnostic(
    code(baudelaire::cli::unset_key),
    help("the built-in default applies; `baudelaire config explain <key>` says what it is for")
)]
pub struct UnsetKey {
    pub key: String,
}
