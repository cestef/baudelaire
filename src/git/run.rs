//! The one place a `git` process is started, under one set of options, in one
//! directory.

use std::io::{BufRead as _, BufReader};
use std::path::Path;
use std::process::{Command, Output, Stdio};

use super::format::{Entry, Format, Pretty};

/// The options every invocation is made under.
///
/// A repository carries its own `.git/config`, and a build reads repositories
/// it did not write. Each setting here either makes git run another program or
/// changes what it prints, so each is pinned rather than trusted:
/// `log.showSignature` would run `gpg` and interleave its output with the log,
/// `core.fsmonitor` runs a program on every `status`, `i18n.logOutputEncoding`
/// re-encodes what a record's fields come back as, and `core.quotepath` decides
/// whether a non-ASCII path arrives escaped.
///
/// `--no-pager` keeps git from starting `core.pager`, and `--no-optional-locks`
/// keeps a read from writing to the repository it is reading.
const OPTIONS: &[&str] = &[
    "--no-pager",
    "--no-optional-locks",
    "-c",
    "core.quotepath=false",
    "-c",
    "core.fsmonitor=false",
    "-c",
    "log.showSignature=false",
    "-c",
    "i18n.logOutputEncoding=UTF-8",
];

/// The environment variables cleared before each invocation.
///
/// The first group redirects git at another repository, an index or an object
/// store, so leaving them set would answer about something other than the
/// directory asked about. The last two name a program for git to run.
const CLEARED: &[&str] = &[
    "GIT_DIR",
    "GIT_WORK_TREE",
    "GIT_COMMON_DIR",
    "GIT_INDEX_FILE",
    "GIT_NAMESPACE",
    "GIT_OBJECT_DIRECTORY",
    "GIT_ALTERNATE_OBJECT_DIRECTORIES",
    "GIT_EXTERNAL_DIFF",
    "GIT_PAGER",
];

/// Why a `git` question went unanswered.
///
/// Distinct from an *empty* answer, which is `Ok("")` and a real one: `git
/// status --porcelain` printing nothing is a clean tree, while a `git` that
/// could not run has said nothing at all. Conflating the two is how a failed
/// status reads as clean.
#[derive(Debug)]
pub(super) enum Unanswered {
    /// `git` is not installed, or the process could not be started.
    Unavailable(std::io::Error),
    /// It ran and refused: not a repository, an unknown revision, a repository
    /// with no commit yet, a `describe` with no tag to name.
    Refused(String),
}

impl std::fmt::Display for Unanswered {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unavailable(why) => write!(f, "git could not be run: {why}"),
            Self::Refused(stderr) if stderr.is_empty() => f.write_str("git refused"),
            Self::Refused(stderr) => write!(f, "git refused: {stderr}"),
        }
    }
}

/// `git`, run in one directory.
///
/// Every argument is a `'static` constant of this module, save the one built
/// from a [`Format`], which is built out of [`super::format::Field`]s. Nothing
/// a site controls can become an argument, so no path and no name can arrive
/// where git would read it as an option.
pub(super) struct Git<'a> {
    /// Where every query runs: the site root rather than the repository root,
    /// so a site that is one directory of a larger repository asks about its
    /// own subtree.
    dir: &'a Path,
}

impl<'a> Git<'a> {
    pub(super) const fn at(dir: &'a Path) -> Self {
        Self { dir }
    }

    /// `git <args>`, as its standard output with trailing newlines removed.
    pub(super) fn read(&self, args: &[&'static str]) -> Result<String, Unanswered> {
        Self::run(self.command(args))
    }

    /// The same, as an optional answer, logging why there is none: a build with
    /// no git state should be able to say what it asked and what it got.
    pub(super) fn ask(&self, args: &[&'static str]) -> Option<String> {
        Self::heard(args, self.read(args))
    }

    /// One commit, as the fields `format` asked for.
    pub(super) fn record<const N: usize>(
        &self,
        args: &[&'static str],
        format: Format<N>,
    ) -> Option<[String; N]> {
        let record = Self::heard(args, Self::run(self.formatted(args, format)))?;
        match Entry::<N>::of(&record) {
            Some(Entry::Commit(fields)) => Some(fields.map(str::to_owned)),
            _ => None,
        }
    }

    /// `git <args> --format=<format>`, handing each line that says something to
    /// `entry` as it arrives.
    ///
    /// Streamed rather than collected: a log holds a line per file per commit,
    /// and what a caller keeps of it is far smaller than the log itself.
    pub(super) fn walk<const N: usize>(
        &self,
        args: &[&'static str],
        format: Format<N>,
        mut entry: impl FnMut(Entry<'_, N>),
    ) -> Result<(), Unanswered> {
        let mut child = self
            .formatted(args, format)
            .stdout(Stdio::piped())
            // Draining two pipes from one thread deadlocks on whichever is not
            // being read; what a walk needs from a failure is that it failed.
            .stderr(Stdio::null())
            .spawn()
            .map_err(Unanswered::Unavailable)?;
        let stdout = child.stdout.take().expect("stdout was piped");
        for line in BufReader::new(stdout).split(b'\n') {
            let Ok(bytes) = line else { break };
            let text = String::from_utf8_lossy(&bytes);
            if let Some(found) = Entry::of(text.trim_end_matches('\r')) {
                entry(found);
            }
        }
        let status = child.wait().map_err(Unanswered::Unavailable)?;
        if status.success() {
            Ok(())
        } else {
            Err(Unanswered::Refused(String::new()))
        }
    }

    fn formatted<const N: usize>(&self, args: &[&'static str], format: Format<N>) -> Command {
        let mut command = self.command(args);
        command.arg(Pretty(format).to_string());
        command
    }

    /// The command git will be run as: the pinned options, then the query, in
    /// the directory being asked about and with nothing inherited that could
    /// redirect it.
    fn command(&self, args: &[&'static str]) -> Command {
        let mut command = Command::new("git");
        command.args(OPTIONS).args(args).current_dir(self.dir);
        for name in CLEARED {
            command.env_remove(name);
        }
        command
    }

    fn run(mut command: Command) -> Result<String, Unanswered> {
        command
            .output()
            .map_err(Unanswered::Unavailable)
            .and_then(|output| Self::decode(&output))
    }

    /// An answer, or nothing and a note saying which question went unanswered.
    fn heard(args: &[&'static str], answer: Result<String, Unanswered>) -> Option<String> {
        match answer {
            Ok(answer) => Some(answer),
            Err(why) => {
                tracing::debug!(command = args.join(" "), "{why}");
                None
            }
        }
    }

    /// Standard output, decoded lossily: git stores a path and an author name
    /// as bytes, and a replacement character in one name is a better answer
    /// than none for the whole repository.
    fn decode(output: &Output) -> Result<String, Unanswered> {
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
            return Err(Unanswered::Refused(stderr));
        }
        let text = String::from_utf8_lossy(&output.stdout);
        Ok(text.trim_end_matches(['\n', '\r']).to_owned())
    }
}
