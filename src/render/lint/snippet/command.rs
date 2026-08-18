//! The command a site names to check a language, and where the snippet it
//! reads is written.

use std::path::{Path, PathBuf};

use crate::config::{Config, Scratch};
use crate::error::Result;
use crate::graph::Hash;
use crate::shell::Shell;

use crate::render::snippet::{Fault, Position, Snippet};

/// A checker that shells out: the snippet is written where the command can read
/// it, and the exit status is what says whether it is good.
pub(super) struct Command<'a> {
    line: &'a str,
    root: &'a Path,
}

impl<'a> Command<'a> {
    /// What the command line stands in for the snippet on disk, as one word.
    const FILE: &'static str = "{file}";
    /// What the command line stands in for the language the fence claimed.
    const LANG: &'static str = "{lang}";
    /// The environment variable holding the binary running this build, so a
    /// site can check its snippets with the very build that is rendering them
    /// rather than with whatever is on `PATH`.
    const SELF: &'static str = "BAUDELAIRE";

    pub(super) fn new(line: &'a str, root: &'a Path) -> Self {
        Self { line, root }
    }

    /// Run it over `snippet`. A command that cannot be spawned, or that fails
    /// without a word about why, still reports: a checker nobody can run is a
    /// finding rather than a silent pass.
    pub(super) fn faults(&self, snippet: &Snippet) -> Vec<Fault> {
        match self.written(snippet) {
            Ok(path) => self.run(snippet, &path),
            Err(why) => vec![why.to_string().into()],
        }
    }

    /// The snippet on disk for the command to read, under the scratch tree and
    /// named by its own digest: the same fence on ten pages is one file, and
    /// `clean` is what removes them.
    fn written(&self, snippet: &Snippet) -> Result<PathBuf> {
        let path = self.root.join(
            Config::scratch(Scratch::Snippets)
                .join(Hash::of(&(&snippet.lang, snippet.text())).hex())
                .with_extension(snippet.extension()),
        );
        crate::fs::write_atomic(&path, snippet.text().as_bytes())?;
        Ok(path)
    }

    fn run(&self, snippet: &Snippet, path: &Path) -> Vec<Fault> {
        let line = self
            .line
            .replace(Self::FILE, &Shell::HOST.word(path))
            .replace(Self::LANG, &snippet.lang);
        let mut command = Shell::HOST.command(&line, self.root);
        if let Ok(running) = std::env::current_exe() {
            command.env(Self::SELF, running);
        }
        let output = match command.output() {
            Ok(output) => output,
            Err(why) => return vec![format!("could not run `{line}`: {why}").into()],
        };
        if output.status.success() {
            return Vec::new();
        }
        let Some(said) = Said::new(&output) else {
            return vec![Silent(&line, output.status).to_string().into()];
        };
        let at = said.position(path);
        vec![Fault {
            at: at.and_then(|at| snippet.offset(at)),
            // With a position parsed, the line that carried it is the whole
            // finding, and the `file:line:column:` it opened with is ours to
            // drop: the finding already points at the fence.
            message: at.map_or_else(|| said.0.to_owned(), |_| said.said().to_owned()),
        }]
    }
}

/// What a checker wrote about the file it was handed: its stderr, or its stdout
/// when it kept stderr quiet.
struct Said<'a>(&'a str);

impl<'a> Said<'a> {
    fn new(output: &'a std::process::Output) -> Option<Self> {
        [&output.stderr, &output.stdout]
            .into_iter()
            .filter_map(|stream| std::str::from_utf8(stream).ok())
            .map(str::trim)
            .find(|said| !said.is_empty())
            .map(Self)
    }

    /// What it said with the position taken off, when it opened with one.
    fn said(&self) -> &str {
        self.0
            .lines()
            .next()
            .and_then(|line| line.rsplit(": ").next())
            .unwrap_or(self.0)
            .trim()
    }

    /// The first position it named in `path`, read off the `path:line:column`
    /// prefix that a checker reporting one opens its line with.
    fn position(&self, path: &Path) -> Option<Position> {
        let names: Vec<&Path> = [Some(path), path.file_name().map(Path::new)]
            .into_iter()
            .flatten()
            .collect();
        self.0.lines().find_map(|line| {
            let rest = names
                .iter()
                .find_map(|name| line.split_once(&format!("{}:", name.display())))?
                .1;
            let mut parts = rest.split(':').map(str::trim).map(str::parse);
            Some(Position {
                line: parts.next()?.ok()?,
                column: parts.next().and_then(Result::ok).unwrap_or(1),
            })
        })
    }
}

/// A checker that failed without saying anything, described by how it ended.
struct Silent<'a>(&'a str, std::process::ExitStatus);

impl std::fmt::Display for Silent<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let Self(line, status) = self;
        match status.code() {
            Some(code) => write!(f, "`{line}` exited with status {code}"),
            None => write!(f, "`{line}` was killed by a signal"),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{Position, Said};

    #[test]
    fn a_reported_position_is_read_off_the_line_the_checker_wrote() {
        let path = Path::new("/tmp/.baudelaire/snippets/ab12.sh");
        for (said, expected) in [
            (
                "/tmp/.baudelaire/snippets/ab12.sh:3:7: bad",
                Some(Position { line: 3, column: 7 }),
            ),
            (
                "ab12.sh:12: unexpected",
                Some(Position {
                    line: 12,
                    column: 1,
                }),
            ),
            ("something went wrong", None),
        ] {
            assert_eq!(Said(said).position(path), expected, "{said}");
        }
    }
}
