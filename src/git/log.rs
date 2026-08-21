//! What the log says about each file: the commit that last touched it, and
//! everyone who ever has.
//!
//! One walk, not a `git log` per page, which on a site of any size is a process
//! per page.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::codegen::Value;

use super::format::{Entry, Field, Format};
use super::run::Git;

/// What the walk asks of each commit.
const COMMIT: Format<4> = Format::new([Field::Hash, Field::Committed, Field::Name, Field::Email]);

/// The log as one walk of it, newest commit first, listing the files each one
/// touched and spelling their paths relative to the site root.
const WALK: &[&str] = &["log", "--name-only", "--no-merges", "--relative"];

/// One commit, as a page's history names it.
#[derive(Hash, Debug, Clone, PartialEq, Eq)]
struct Commit {
    hash: String,
    /// The committer date, ISO-8601.
    at: String,
    author: Author,
}

/// Who made a change.
#[derive(Hash, Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct Author {
    name: String,
    email: String,
}

/// Someone who has changed a file, and how many commits of theirs touched it.
#[derive(Hash, Debug, Clone, PartialEq, Eq)]
struct Contributor {
    author: Author,
    commits: u32,
}

/// One file's history.
#[derive(Hash, Debug, Clone, PartialEq, Eq)]
pub struct Changed {
    /// The commit that last touched it, which is what a *last updated* line and
    /// a sitemap's `lastmod` read.
    last: Commit,
    /// Everyone who has, most commits first and then by name. Empty unless
    /// `content { history { contributors } }` asked for them.
    contributors: Vec<Contributor>,
}

impl Changed {
    /// When the file last changed, ISO-8601, for a surface that wants the date
    /// and not the commit.
    pub fn committed(&self) -> &str {
        &self.last.at
    }

    /// Credit `author` with one more commit against this file.
    fn credit(&mut self, author: &Author) {
        match self
            .contributors
            .iter_mut()
            .find(|held| held.author == *author)
        {
            Some(held) => held.commits = held.commits.saturating_add(1),
            None => self.contributors.push(Contributor {
                author: author.clone(),
                commits: 1,
            }),
        }
    }

    /// Order this file's contributors by how much of it is theirs, then by
    /// name, so two with the same count come back the same way on every build.
    fn rank(&mut self) {
        self.contributors.sort_by(|a, b| {
            b.commits
                .cmp(&a.commits)
                .then_with(|| a.author.cmp(&b.author))
        });
    }
}

/// What the log says about the files under one site root, keyed the way
/// [`crate::graph::Portable`] keys a path.
#[derive(Hash, Debug, Default)]
pub struct History(BTreeMap<PathBuf, Changed>);

impl History {
    /// The empty history, which is what a build outside a repository and a
    /// build that never asked for one both have.
    pub fn none() -> &'static Self {
        static NONE: std::sync::LazyLock<History> = std::sync::LazyLock::new(History::default);
        &NONE
    }

    /// Walk the log once.
    ///
    /// Empty for a repository with no commit yet, and for one the walk could
    /// not read: a history nobody can read is a page without one, never a
    /// failed build.
    pub(super) fn read(git: &Git, contributors: bool) -> Self {
        let mut walk = Walk::new(contributors);
        if let Err(why) = git.walk(WALK, COMMIT, |entry| walk.entry(&entry)) {
            tracing::debug!("git log: {why}");
        }
        walk.finish()
    }

    /// What the log says about `path`, or `None` for a file no commit has
    /// touched: a page nobody has committed yet, or a build outside a
    /// repository.
    pub fn of(&self, path: &Path) -> Option<&Changed> {
        self.0.get(path)
    }
}

/// One walk in progress: the commit whose files are being read, and what each
/// file has been told so far.
struct Walk {
    contributors: bool,
    /// The commit the lines now arriving belong to, absent before the first one
    /// and for a record that did not parse.
    open: Option<Commit>,
    changed: BTreeMap<PathBuf, Changed>,
}

impl Walk {
    const fn new(contributors: bool) -> Self {
        Self {
            contributors,
            open: None,
            changed: BTreeMap::new(),
        }
    }

    /// One line the walk said something about, destructured against
    /// [`COMMIT`]: the fields asked for and the fields read are one statement,
    /// and their number is checked where it is written.
    fn entry(&mut self, entry: &Entry<'_, 4>) {
        match entry {
            Entry::Commit([hash, at, name, email]) => {
                self.open = Some(Commit {
                    hash: (*hash).to_owned(),
                    at: (*at).to_owned(),
                    author: Author {
                        name: (*name).to_owned(),
                        email: (*email).to_owned(),
                    },
                });
            }
            Entry::Touched(path) => self.touched(path),
        }
    }

    /// Record that the open commit touched `path`.
    ///
    /// The log runs newest first, so the first commit to name a file is the one
    /// that last changed it and every later mention only adds to its credits.
    fn touched(&mut self, path: &Path) {
        let Some(commit) = self.open.as_ref() else {
            return;
        };
        let changed = self
            .changed
            .entry(path.to_path_buf())
            .or_insert_with(|| Changed {
                last: commit.clone(),
                contributors: Vec::new(),
            });
        if self.contributors {
            changed.credit(&commit.author);
        }
    }

    fn finish(mut self) -> History {
        for changed in self.changed.values_mut() {
            changed.rank();
        }
        History(self.changed)
    }
}

/// A page's `page.git`.
impl From<&Changed> for Value {
    fn from(changed: &Changed) -> Self {
        let mut fields = vec![
            ("hash", Self::str(&changed.last.hash)),
            ("committed", Self::str(&changed.last.at)),
            ("author", Self::from(&changed.last.author)),
        ];
        if !changed.contributors.is_empty() {
            fields.push((
                "contributors",
                Self::array(changed.contributors.iter().map(Self::from)),
            ));
        }
        Self::dict(fields)
    }
}

impl From<&Author> for Value {
    fn from(author: &Author) -> Self {
        Self::dict([
            ("name", Self::str(&author.name)),
            ("email", Self::str(&author.email)),
        ])
    }
}

impl From<&Contributor> for Value {
    fn from(contributor: &Contributor) -> Self {
        Self::dict([
            ("name", Self::str(&contributor.author.name)),
            ("email", Self::str(&contributor.author.email)),
            ("commits", Self::Int(i64::from(contributor.commits))),
        ])
    }
}

#[cfg(test)]
mod tests {
    use super::{Author, Entry, Walk};
    use std::path::{Path, PathBuf};

    /// A commit as the walk is handed one, with the fields [`super::COMMIT`]
    /// asks for.
    fn commit<'a>(hash: &'a str, at: &'a str, name: &'a str) -> Entry<'a, 4> {
        Entry::Commit([hash, at, name, "who@example.com"])
    }

    fn touched(path: &str) -> Entry<'static, 4> {
        Entry::Touched(PathBuf::from(path))
    }

    fn walk(contributors: bool, entries: &[Entry<'_, 4>]) -> super::History {
        let mut walk = Walk::new(contributors);
        for entry in entries {
            walk.entry(entry);
        }
        walk.finish()
    }

    /// The log runs newest first, so the first commit to name a file is the one
    /// that last changed it.
    #[test]
    fn a_file_takes_the_first_commit_that_names_it() {
        let history = walk(
            false,
            &[
                commit("new", "2026-08-21T10:00:00Z", "Ada"),
                touched("a.typ"),
                commit("old", "2026-08-01T10:00:00Z", "Bo"),
                touched("a.typ"),
                touched("b.typ"),
            ],
        );

        assert_eq!(
            history.of(Path::new("a.typ")).unwrap().committed(),
            "2026-08-21T10:00:00Z"
        );
        assert_eq!(
            history.of(Path::new("b.typ")).unwrap().committed(),
            "2026-08-01T10:00:00Z"
        );
        assert!(history.of(Path::new("c.typ")).is_none());
    }

    #[test]
    fn contributors_are_gathered_only_when_they_are_asked_for() {
        let entries = [
            commit("c", "2026-08-21T10:00:00Z", "Ada"),
            touched("a.typ"),
            commit("b", "2026-08-20T10:00:00Z", "Bo"),
            touched("a.typ"),
            commit("a", "2026-08-19T10:00:00Z", "Bo"),
            touched("a.typ"),
        ];

        let without = walk(false, &entries);
        assert!(
            without
                .of(Path::new("a.typ"))
                .unwrap()
                .contributors
                .is_empty()
        );

        let with = walk(true, &entries);
        assert_eq!(
            with.of(Path::new("a.typ"))
                .unwrap()
                .contributors
                .iter()
                .map(|c| (c.author.name.as_str(), c.commits))
                .collect::<Vec<_>>(),
            [("Bo", 2), ("Ada", 1)],
            "most commits first"
        );
    }

    /// Two authors with the same count come back by name, so a build's output
    /// does not depend on the order git happened to walk in.
    #[test]
    fn a_tie_between_contributors_is_broken_by_name() {
        let history = walk(
            true,
            &[
                commit("b", "2026-08-21T10:00:00Z", "Zoe"),
                touched("a.typ"),
                commit("a", "2026-08-20T10:00:00Z", "Ada"),
                touched("a.typ"),
            ],
        );

        assert_eq!(
            history
                .of(Path::new("a.typ"))
                .unwrap()
                .contributors
                .iter()
                .map(|c| c.author.name.as_str())
                .collect::<Vec<_>>(),
            ["Ada", "Zoe"]
        );
    }

    /// A path before any commit belongs to none, and is not history.
    #[test]
    fn a_path_with_no_commit_open_is_dropped() {
        let history = walk(false, &[touched("a.typ")]);

        assert!(history.of(Path::new("a.typ")).is_none());
    }

    #[test]
    fn an_author_orders_by_name_before_email() {
        let mut authors = [
            Author {
                name: "Bo".into(),
                email: "a@x".into(),
            },
            Author {
                name: "Ada".into(),
                email: "z@x".into(),
            },
        ];
        authors.sort();

        assert_eq!(authors[0].name, "Ada");
    }
}
