//! What `HEAD` is: the commit a build was made from, and whether the tree it
//! was made from matched that commit.

use crate::codegen::Value;

use super::format::{Field, Format};
use super::run::Git;

/// What a build reads off the commit itself.
const COMMIT: Format<2> = Format::new([Field::Hash, Field::Committed]);

/// Git state of the site's repository at build time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Head {
    /// The full commit SHA.
    pub hash: String,
    /// Its committer date, ISO-8601.
    pub committed: String,
    /// Whether the working tree carried changes that commit does not.
    ///
    /// The whole repository's, not the site's subtree: `hash` and `branch` are
    /// the whole repository's too, and a build cannot be half-described.
    pub dirty: bool,
    /// How many commits are reachable from it.
    pub rev: Option<u64>,
    /// The branch checked out, absent on a detached `HEAD`.
    pub branch: Option<String>,
    /// The nearest tag, absent in a repository that has none.
    pub tag: Option<String>,
}

impl Head {
    /// What `HEAD` is, or `None` for a repository with no commit yet.
    ///
    /// The first three fields are what a build *is*, so a query for one of them
    /// going unanswered means there is no git state to report rather than a
    /// state with holes in it. The last three are genuinely absent in ordinary
    /// repositories, and each is its own `Option`.
    ///
    /// `describe` deliberately omits `--always`, which reports a bare commit
    /// hash in a tagless repository and would fill `tag` with something that is
    /// not one.
    pub(super) fn read(git: &Git) -> Option<Self> {
        let [hash, committed] = git.record(&["log", "-1"], COMMIT)?;
        Some(Self {
            hash,
            committed,
            dirty: !git.ask(&["status", "--porcelain"])?.is_empty(),
            rev: git
                .ask(&["rev-list", "--count", "HEAD"])
                .and_then(|count| count.parse().ok()),
            branch: git.ask(&["symbolic-ref", "--quiet", "--short", "HEAD"]),
            tag: git.ask(&["describe", "--tags"]),
        })
    }
}

/// The `git` half of `sys.inputs.baudelaire`.
impl From<&Head> for Value {
    fn from(head: &Head) -> Self {
        let mut fields = vec![("hash", Self::str(&head.hash))];
        if let Some(rev) = head.rev {
            fields.push(("rev", Self::str(rev.to_string())));
        }
        if let Some(branch) = &head.branch {
            fields.push(("branch", Self::str(branch)));
        }
        if let Some(tag) = &head.tag {
            fields.push(("tag", Self::str(tag)));
        }
        fields.push(("committed", Self::str(&head.committed)));
        fields.push(("dirty", Self::Bool(head.dirty)));
        Self::dict(fields)
    }
}
