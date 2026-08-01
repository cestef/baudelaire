//! Errors from the commands that describe the CLI itself rather than build a
//! site: `completions` and `man`.

use miette::Diagnostic;
use thiserror::Error;

use crate::ui::Code;

/// What was being written when a generated document failed to reach stdout.
///
/// Carried rather than folded into the message so the diagnostic code stays
/// stable while the wording is free to change, the same split [`crate::error::Op`]
/// makes for filesystem operations.
#[derive(Debug, Clone, Copy)]
pub enum Generated {
    Completions,
    Man,
    Reference,
}

impl Generated {
    /// The command that produces it, which is also what the help suggests
    /// retrying, so the two cannot disagree.
    const fn command(self) -> &'static str {
        match self {
            Self::Completions => "baudelaire completions",
            Self::Man => "baudelaire man",
            Self::Reference => "baudelaire reference",
        }
    }

    const fn what(self) -> &'static str {
        match self {
            Self::Completions => "completion script",
            Self::Man => "man page",
            Self::Reference => "config reference",
        }
    }

    /// Write this document to stdout as one call, and flush it.
    ///
    /// The flush is the point: stdout is block-buffered when redirected to a
    /// file, so a write that only reaches the buffer reports success and then
    /// loses the tail if the implicit flush at exit fails.
    pub fn emit(self, bytes: &[u8]) -> Result<(), WriteFailed> {
        use std::io::Write;

        let mut out = std::io::stdout().lock();
        self.check(out.write_all(bytes).and_then(|()| out.flush()))
    }

    /// Fold a write result into "the reader went away, which is fine" or a real
    /// error, in the one place every step of both commands shares.
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

/// A generated document could not be written to stdout.
///
/// Not [`crate::error::BaudelaireErrorKind::Terminal`]: that variant is
/// documented as the last blanket `io::Error` conversion and as a finding to be
/// removed, so a new caller of it would be moving the wrong way.
///
/// A broken pipe never reaches here. `baudelaire man | head` closes the reader
/// mid-write, which is the reader's normal behaviour and not this command's
/// failure, so the callers treat it as success; what is left is a real write
/// failure, most often a full disk on a redirect.
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

/// `baudelaire reference <key>` was given a path the config has no such key at.
///
/// The suggestion comes from the same walk that would have printed it, so the
/// names offered are exactly the ones that would work.
#[derive(Debug, Error, Diagnostic)]
#[error("no config key at {}", Code(.key))]
#[diagnostic(code(baudelaire::cli::unknown_key), help("{help}"))]
pub struct UnknownKey {
    pub key: String,
    pub help: String,
}
