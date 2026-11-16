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
    /// The project root as the author spelled it, made absolute and never
    /// canonicalized: on Windows the canonical form is a `\\?\` path most editors
    /// cannot open.
    home: PathBuf,
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
            home: std::path::absolute(&root).unwrap_or_else(|_| root.clone()),
            root: crate::fs::canonicalize(&root).unwrap_or(root),
            command: config.serve.editor.clone(),
        })
    }

    /// Run the editor at `at`, once the file it names is confirmed to be one of
    /// the project's own.
    pub(super) fn at(&self, at: &At<'_>) -> Result<(), Unopenable> {
        let path = self
            .resolve(at.file)
            .ok_or_else(|| Unopenable::Outside(at.file.to_owned()))?;
        let mut words = self.substituted(&path.display().to_string(), at);
        let program = words.remove(0);
        let mut child = Command::new(&program)
            .args(words)
            .current_dir(&self.home)
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

    /// The project file `file` names, spelled under [`Open::home`], or `None`
    /// when it is not a file inside the project.
    ///
    /// The containment check runs on canonical paths, so a symlink cannot lead
    /// out; what is returned is the same file without the canonical spelling.
    fn resolve(&self, file: &str) -> Option<PathBuf> {
        let real = crate::fs::canonicalize(self.root.join(file)).ok()?;
        let inside = real.strip_prefix(&self.root).ok()?;
        real.is_file().then(|| self.home.join(inside))
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

#[cfg(all(test, unix))]
mod tests {
    use super::Open;

    fn open(root: &std::path::Path, home: &std::path::Path) -> Open {
        Open {
            root: crate::fs::canonical(root),
            home: home.to_path_buf(),
            command: vec!["true".into()],
        }
    }

    #[test]
    fn a_file_is_spelled_under_the_root_as_the_author_wrote_it() {
        let tmp = tempfile::tempdir().unwrap();
        let real = tmp.path().join("real");
        std::fs::create_dir_all(real.join("content")).unwrap();
        std::fs::write(real.join("content/a.typ"), "x").unwrap();
        let link = tmp.path().join("link");
        std::os::unix::fs::symlink(&real, &link).unwrap();

        let found = open(&link, &link).resolve("content/a.typ");

        assert_eq!(found, Some(link.join("content/a.typ")));
    }

    #[test]
    fn a_symlink_leading_out_of_the_project_is_refused() {
        let tmp = tempfile::tempdir().unwrap();
        let (root, outside) = (tmp.path().join("root"), tmp.path().join("outside"));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::write(outside.join("secret.typ"), "x").unwrap();
        std::os::unix::fs::symlink(&outside, root.join("escape")).unwrap();

        assert_eq!(open(&root, &root).resolve("escape/secret.typ"), None);
    }
}
