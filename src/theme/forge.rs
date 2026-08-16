//! A theme from a repository on a forge, fetched as the forge's own source
//! archive of one revision: a table of URL shapes over the [`archive`] source.
//!
//! [`archive`]: super::archive

use super::archive::Archive;
use super::source::{Fetched, Fetching, Origin, Source};
use crate::error::{Result, ThemeError};

/// A repository on a forge, fetched as a source archive.
pub struct Repository;

/// One forge: how a repository on it is named, and how it spells an archive of
/// one revision.
struct Forge {
    /// What a spec says: `gh:owner/repo`, or `forgejo:host/owner/repo`.
    key: &'static str,
    /// The instance this key means, or `None` when the spec names one.
    host: Option<&'static str>,
    kind: Kind,
}

/// How a forge spells an archive URL, one arm per URL shape rather than per
/// forge.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    GitHub,
    GitLab,
    Forgejo,
    SourceHut,
}

/// The forges a repository can be named on: the keys a spec may use, the hosts
/// a URL is recognised by, and the archive URL each produces all read this one
/// table.
const FORGES: &[Forge] = &[
    Forge {
        key: "gh",
        host: Some("github.com"),
        kind: Kind::GitHub,
    },
    Forge {
        key: "gl",
        host: Some("gitlab.com"),
        kind: Kind::GitLab,
    },
    Forge {
        key: "cb",
        host: Some("codeberg.org"),
        kind: Kind::Forgejo,
    },
    Forge {
        key: "sr",
        host: Some("git.sr.ht"),
        kind: Kind::SourceHut,
    },
    Forge {
        key: "github",
        host: None,
        kind: Kind::GitHub,
    },
    Forge {
        key: "gitlab",
        host: None,
        kind: Kind::GitLab,
    },
    Forge {
        key: "forgejo",
        host: None,
        kind: Kind::Forgejo,
    },
    Forge {
        key: "gitea",
        host: None,
        kind: Kind::Forgejo,
    },
    Forge {
        key: "sourcehut",
        host: None,
        kind: Kind::SourceHut,
    },
];

/// A repository, resolved to the three things an archive URL needs.
struct Named<'a> {
    kind: Kind,
    host: &'a str,
    owner: &'a str,
    repo: &'a str,
}

impl Named<'_> {
    /// The forge's archive of `r#ref`, as that forge spells it.
    fn archive(&self, r#ref: &str) -> String {
        let Self {
            kind,
            host,
            owner,
            repo,
        } = self;
        match kind {
            Kind::GitHub | Kind::Forgejo => {
                format!("https://{host}/{owner}/{repo}/archive/{ref}.tar.gz")
            }
            Kind::GitLab => {
                format!("https://{host}/{owner}/{repo}/-/archive/{ref}/{repo}-{ref}.tar.gz")
            }
            Kind::SourceHut => {
                let owner = owner.trim_start_matches('~');
                format!("https://{host}/~{owner}/{repo}/archive/{ref}.tar.gz")
            }
        }
    }
}

impl Repository {
    /// The revision fetched when a spec names none.
    const HEAD: &'static str = "HEAD";

    /// Split a spec into the repository and the revision it names.
    fn split(spec: &str) -> (&str, Option<&str>) {
        match spec.split_once('#') {
            Some((repo, r#ref)) if !r#ref.is_empty() => (repo, Some(r#ref)),
            _ => (spec, None),
        }
    }

    /// Read a repository spec: a forge key and a path, or a URL on a forge this
    /// knows. Anything else is not a repository as far as this is concerned.
    fn named(repo: &str) -> Option<Named<'_>> {
        let (head, rest) = repo.split_once(':')?;
        if head == "https" || head == "http" {
            let path = rest.trim_start_matches('/');
            let (host, rest) = path.split_once('/')?;
            let forge = FORGES
                .iter()
                .find(|forge| forge.host == Some(host) && forge.key.len() == 2)?;
            let (owner, repo) = Self::segments(rest)?;
            return Some(Named {
                kind: forge.kind,
                host,
                owner,
                repo,
            });
        }
        let forge = FORGES.iter().find(|forge| forge.key == head)?;
        let (host, rest) = match forge.host {
            Some(host) => (host, rest),
            None => rest.trim_start_matches('/').split_once('/')?,
        };
        let (owner, repo) = Self::segments(rest)?;
        Some(Named {
            kind: forge.kind,
            host,
            owner,
            repo,
        })
    }

    /// The `owner/repo` at the end of a spec, with the `.git` a URL may carry
    /// dropped. Exactly two segments, and neither `.` nor `..`: the repository
    /// name becomes the installed directory's name.
    fn segments(rest: &str) -> Option<(&str, &str)> {
        let rest = rest.trim_end_matches('/').trim_end_matches(".git");
        let (owner, repo) = rest.split_once('/')?;
        let named = |s: &str| !s.is_empty() && s != "." && s != "..";
        (named(owner) && named(repo) && !repo.contains('/')).then_some((owner, repo))
    }
}

impl Source for Repository {
    fn name(&self) -> &'static str {
        "forge"
    }

    /// A forge shorthand or a URL on a known forge, with an optional `#ref`
    /// naming a tag, a branch or a commit.
    ///
    /// Must be offered a spec after the archive source, which claims the URLs
    /// that name an archive directly.
    fn parse(&self, spec: &str) -> Option<Origin> {
        let (repo, r#ref) = Self::split(spec);
        Self::named(repo)?;
        Some(Origin::Forge {
            repo: repo.to_owned(),
            r#ref: r#ref.map(ToOwned::to_owned),
            resolved: None,
            subdir: None,
        })
    }

    fn owns(&self, origin: &Origin) -> bool {
        matches!(origin, Origin::Forge { .. })
    }

    fn fetch(&self, origin: &Origin, _cx: &Fetching) -> Result<Fetched> {
        let Origin::Forge {
            repo,
            r#ref,
            subdir,
            ..
        } = origin
        else {
            return Err(ThemeError::unsupported(origin.label()).into());
        };
        let named = Self::named(repo).ok_or_else(|| ThemeError::unsupported(repo.clone()))?;
        let url = named.archive(r#ref.as_deref().unwrap_or(Self::HEAD));
        let (wrapper, files) = Archive::contents(&url)?;
        if files.is_empty() {
            return Err(ThemeError::empty(&url).into());
        }
        Ok(Fetched {
            name: named.repo.to_owned(),
            about: None,
            origin: Origin::Forge {
                repo: repo.clone(),
                r#ref: r#ref.clone(),
                resolved: wrapper,
                subdir: subdir.clone(),
            },
            files,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn url_of(spec: &str) -> Option<String> {
        let (repo, r#ref) = Repository::split(spec);
        let named = Repository::named(repo)?;
        Some(named.archive(r#ref.unwrap_or(Repository::HEAD)))
    }

    #[test]
    fn a_shorthand_becomes_that_forge_s_archive_url() {
        assert_eq!(
            url_of("gh:cestef/plume#v1.2.0").as_deref(),
            Some("https://github.com/cestef/plume/archive/v1.2.0.tar.gz")
        );
        assert_eq!(
            url_of("cb:cestef/plume").as_deref(),
            Some("https://codeberg.org/cestef/plume/archive/HEAD.tar.gz")
        );
        assert_eq!(
            url_of("gl:group/plume#main").as_deref(),
            Some("https://gitlab.com/group/plume/-/archive/main/plume-main.tar.gz")
        );
        assert_eq!(
            url_of("sr:sircmpwn/plume").as_deref(),
            Some("https://git.sr.ht/~sircmpwn/plume/archive/HEAD.tar.gz")
        );
        assert_eq!(url_of("sr:~sircmpwn/plume"), url_of("sr:sircmpwn/plume"));
    }

    #[test]
    fn a_url_on_a_known_forge_is_recognised() {
        assert_eq!(
            url_of("https://github.com/cestef/plume.git").as_deref(),
            Some("https://github.com/cestef/plume/archive/HEAD.tar.gz")
        );
        assert_eq!(
            url_of("https://codeberg.org/cestef/plume/").as_deref(),
            Some("https://codeberg.org/cestef/plume/archive/HEAD.tar.gz")
        );
        assert!(url_of("https://git.example.net/cestef/plume").is_none());
    }

    #[test]
    fn a_software_key_takes_the_host_from_the_spec() {
        assert_eq!(
            url_of("forgejo:git.example.net/cestef/plume#v1").as_deref(),
            Some("https://git.example.net/cestef/plume/archive/v1.tar.gz")
        );
        assert_eq!(
            url_of("gitea:git.example.net/cestef/plume"),
            url_of("forgejo:git.example.net/cestef/plume")
        );
        assert_eq!(
            url_of("gitlab:gl.example.net/team/plume#next").as_deref(),
            Some("https://gl.example.net/team/plume/-/archive/next/plume-next.tar.gz")
        );
    }

    #[test]
    fn only_a_repository_is_claimed() {
        for spec in [
            "plume",
            "./plume",
            "@preview/plume:1.0.0",
            "gh:cestef",
            "gh:cestef/plume/themes/x",
            "nope:cestef/plume",
        ] {
            assert!(Repository.parse(spec).is_none(), "{spec}");
        }
    }

    #[test]
    fn a_ref_is_split_off_the_spec() {
        let Some(Origin::Forge { repo, r#ref, .. }) = Repository.parse("gh:cestef/plume#v1.2.0")
        else {
            panic!("not a repository");
        };
        assert_eq!(repo, "gh:cestef/plume");
        assert_eq!(r#ref.as_deref(), Some("v1.2.0"));
    }
}
