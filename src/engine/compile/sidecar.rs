//! Sidecars: the files a page produces beside its HTML, drawn by a second,
//! *paged* compile of the same page.

use std::path::PathBuf;

use crate::config::Config;
use crate::content::Page;
use crate::error::Result;
use crate::graph::Deps;

use super::prepare::Prepare;

#[cfg(feature = "sidecars")]
use {
    super::paged::{Laid, Paged},
    crate::world::Project,
    typst::syntax::RootedPath,
};

#[cfg(not(feature = "sidecars"))]
use crate::world::Project;

/// What a sidecar builds its module out of: the site config, and the same
/// per-page bindings the HTML compile is given.
#[cfg(feature = "sidecars")]
pub(in crate::engine) struct Cx<'a> {
    pub config: &'a Config,
    pub prepare: &'a Prepare<'a>,
}

/// One drawn sidecar file, ready to write.
pub(in crate::engine) struct Artifact {
    /// The [`Sidecar`] that drew it.
    pub kind: &'static str,
    /// Where it lands under `dist`.
    pub path: PathBuf,
    pub bytes: Vec<u8>,
}

/// A kind of paged artifact a page can produce beside its HTML.
#[cfg(feature = "sidecars")]
pub(in crate::engine) trait Sidecar: Sync {
    /// This kind's name, in one word: the synthetic module's file id, the label
    /// a compile error is reported against, and the noun the summary counts.
    fn name(&self) -> &'static str;

    /// Whether this page gets one; read by the prune too, so a file an earlier
    /// build wrote is never swept out from under a cache hit.
    fn wanted(&self, config: &Config, page: &Page) -> bool;

    /// Where this page's artifact lands under `dist`.
    fn path(&self, config: &Config, page: &Page) -> PathBuf;

    /// The synthetic Typst module to compile: what binds this page to whatever
    /// template the artifact is drawn from.
    fn source(&self, cx: &Cx<'_>, page: &Page, rooted: &RootedPath) -> Result<String>;

    /// Encode the laid-out document into the bytes that get written.
    fn encode(&self, laid: &Laid, page: &Page) -> Result<Vec<u8>>;

    /// Lay this page's module out and encode it, reporting what the compile
    /// read.
    fn draw(&self, project: &Project, cx: &Cx<'_>, page: &Page) -> Result<(Artifact, Deps)> {
        let rooted = project.virtualize(&page.source)?;
        let laid = Paged {
            name: Paged::of(&rooted),
            kind: self.name(),
            text: self.source(cx, page, &rooted)?,
        }
        .run(project)?;
        let artifact = Artifact {
            kind: self.name(),
            path: self.path(cx.config, page),
            bytes: self.encode(&laid, page)?,
        };
        Ok((artifact, laid.deps))
    }
}

#[cfg(feature = "sidecars")]
pub(in crate::engine) struct Sidecars(Vec<Box<dyn Sidecar>>);

/// Without the paged compile there is nothing to register.
#[cfg(not(feature = "sidecars"))]
pub(in crate::engine) struct Sidecars;

#[cfg(feature = "sidecars")]
impl Sidecars {
    pub(in crate::engine) fn builtin() -> Self {
        Self(vec![
            #[cfg(feature = "cards")]
            Box::new(super::card::Card),
            #[cfg(feature = "pdf")]
            Box::new(super::pdf::Pdf),
        ])
    }

    /// A registry that draws nothing, for the backlink repair pass, which only
    /// wants the page's markup again.
    pub(in crate::engine) fn none() -> Self {
        Self(Vec::new())
    }

    /// Draw every sidecar this page wants, with the merged dependency set of
    /// their compiles.
    pub(in crate::engine) fn draw(
        &self,
        project: &Project,
        config: &Config,
        prepare: &Prepare<'_>,
        page: &Page,
    ) -> Result<(Vec<Artifact>, Deps)> {
        let cx = Cx { config, prepare };
        let mut artifacts = Vec::new();
        let mut deps = Deps::default();
        for sidecar in self.0.iter().filter(|s| s.wanted(config, page)) {
            let (artifact, read) = sidecar.draw(project, &cx, page)?;
            deps.extend(read.files().iter().cloned());
            artifacts.push(artifact);
        }
        Ok((artifacts, deps))
    }

    /// Every file this page's sidecars own, whether or not this build drew
    /// them, since a cache hit draws nothing and the prune reads this to keep
    /// what an earlier build left.
    pub(in crate::engine) fn planned(&self, config: &Config, page: &Page) -> Vec<PathBuf> {
        self.0
            .iter()
            .filter(|s| s.wanted(config, page))
            .map(|s| s.path(config, page))
            .collect()
    }
}

// The stubs mirror the `sidecars`-on signatures exactly, so the caller compiles
// unchanged in both flavors.
#[cfg(not(feature = "sidecars"))]
#[allow(clippy::unused_self, clippy::unnecessary_wraps)]
impl Sidecars {
    pub(in crate::engine) fn builtin() -> Self {
        Self
    }

    pub(in crate::engine) fn none() -> Self {
        Self
    }

    pub(in crate::engine) fn draw(
        &self,
        _project: &Project,
        _config: &Config,
        _prepare: &Prepare<'_>,
        _page: &Page,
    ) -> Result<(Vec<Artifact>, Deps)> {
        Ok((Vec::new(), Deps::default()))
    }

    pub(in crate::engine) fn planned(&self, _config: &Config, _page: &Page) -> Vec<PathBuf> {
        Vec::new()
    }
}

#[cfg(all(test, feature = "sidecars"))]
mod tests {
    use super::*;

    #[test]
    fn every_registered_sidecar_names_a_distinct_kind() {
        let sidecars = Sidecars::builtin();
        for (i, sidecar) in sidecars.0.iter().enumerate() {
            assert!(
                !sidecars.0[i + 1..]
                    .iter()
                    .any(|other| other.name() == sidecar.name()),
                "`{}` is claimed by two sidecars",
                sidecar.name()
            );
        }
    }
}

/// How many sidecar files a build drew, by kind, and what they came to.
#[derive(Default)]
pub(in crate::engine) struct Tally {
    /// One entry per kind that drew at least one file, in the order the kinds
    /// were first seen.
    pub kinds: Vec<(&'static str, usize)>,
    pub bytes: u64,
}

impl Tally {
    /// Count what this build drew; a cache hit draws nothing, so a file an
    /// earlier build left is not counted.
    pub(in crate::engine) fn of<'a>(artifacts: impl IntoIterator<Item = &'a Artifact>) -> Self {
        let mut tally = Self::default();
        for artifact in artifacts {
            match tally
                .kinds
                .iter_mut()
                .find(|(kind, _)| *kind == artifact.kind)
            {
                Some((_, count)) => *count += 1,
                None => tally.kinds.push((artifact.kind, 1)),
            }
            tally.bytes += artifact.bytes.len() as u64;
        }
        tally
    }
}
