//! Opening the served site in a browser, and saying so when it cannot.

use std::path::PathBuf;
use std::process::{Command, Stdio};

use super::route::At;
use crate::config::Config;
use crate::error::Result;

/// Opens a source location in the author's editor, for a preview alt-click.
/// Exists only when `serve { editor .. }` names a command; nothing is guessed
/// from the environment.
#[derive(Clone)]
pub(super) struct Open {
    /// The project root, canonical; every requested file must resolve inside it.
    root: PathBuf,
    /// The program, then each of its arguments, run directly and never through
    /// a shell.
    command: Vec<String>,
}

impl Open {
    /// Endpoint the injected client posts a location to.
    pub(super) const ENDPOINT: &'static str = open_endpoint!();

    pub(super) fn new(config: &Config) -> Option<Self> {
        let root = config.root.clone();
        (!config.serve.editor.is_empty()).then(|| Self {
            root: crate::fs::canonicalize(&root).unwrap_or(root),
            command: config.serve.editor.clone(),
        })
    }

    /// Run the editor at `at`, once the file it names is confirmed to be one of
    /// the project's own.
    pub(super) fn at(&self, at: &At<'_>) -> Result<(), Unopenable> {
        let path = crate::fs::canonicalize(self.root.join(at.file))
            .ok()
            .filter(|path| path.starts_with(&self.root) && path.is_file())
            .ok_or_else(|| Unopenable::Outside(at.file.to_owned()))?;
        let mut words = self.substituted(&path.display().to_string(), at);
        let program = words.remove(0);
        let mut child = Command::new(&program)
            .args(words)
            .current_dir(&self.root)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|source| Unopenable::Spawn {
                program: program.clone(),
                source,
            })?;
        std::thread::spawn(move || {
            let _ = child.wait();
        });
        Ok(())
    }

    /// The command with `{file}`, `{line}` and `{column}` filled in, per word,
    /// so a value never splits an argument in two.
    fn substituted(&self, file: &str, at: &At<'_>) -> Vec<String> {
        self.command
            .iter()
            .map(|word| {
                word.replace("{file}", file)
                    .replace("{line}", &at.line.to_string())
                    .replace("{column}", &at.column.to_string())
            })
            .collect()
    }
}

/// Why a source location could not be opened. Its text is the response body, so
/// it addresses the author at the browser: what to fix, not what failed inside.
#[derive(thiserror::Error, Debug)]
pub(super) enum Unopenable {
    #[error("refused: this endpoint only answers the page it was served with")]
    Foreign,
    #[error("no editor configured")]
    Unconfigured,
    /// The request named no location at all.
    #[error("no source location: this endpoint takes `?at=file:line:column`")]
    Unaddressed,
    #[error("not a source location: {0}")]
    Malformed(String),
    #[error("{0} is not a file in this project")]
    Outside(String),
    #[error("could not run {program}: {source}")]
    Spawn {
        program: String,
        source: std::io::Error,
    },
}

impl Unopenable {
    /// The status the refusal answers with: a browser distinguishes them, and
    /// the client shows the body either way.
    pub(super) fn status(&self) -> u16 {
        match self {
            Self::Foreign => 403,
            Self::Unconfigured => 501,
            Self::Unaddressed | Self::Malformed(_) => 400,
            Self::Outside(_) => 404,
            Self::Spawn { .. } => 500,
        }
    }

    /// What to do about it, when there is something to do.
    fn help(&self) -> Option<&'static str> {
        match self {
            Self::Foreign => None,
            Self::Unconfigured => Some(
                "add `serve { editor \"code\" \"--goto\" \"{file}:{line}:{column}\" }` to config.kdl",
            ),
            Self::Unaddressed => Some(
                "the alt-click handler sends it; nothing else has a reason to call this endpoint",
            ),
            Self::Malformed(_) => Some("a stamped element reads `file:line:column`"),
            Self::Outside(_) => Some("only files under the project root can be opened"),
            Self::Spawn { source, .. } => match source.kind() {
                std::io::ErrorKind::NotFound => {
                    Some("is it installed, and on the PATH this dev server inherited?")
                }
                _ => None,
            },
        }
    }

    /// The response body, in the shape a diagnostic has: a marked headline and
    /// a `help:` line, which the injected client renders as it renders a failed
    /// rebuild.
    pub(super) fn body(&self) -> String {
        use std::fmt::Write as _;

        let mut text = format!("× {self}");
        if let Some(help) = self.help() {
            let _ = write!(text, "\nhelp: {help}");
        }
        text
    }
}
