//! Sidecars: the files a page produces beside its HTML, drawn by a second,
//! *paged* compile of the same page.
//!
//! A page's own compile targets HTML, where `html.elem` exists and page layout
//! does not. A sidecar targets the other half: ordinary Typst, laid out on real
//! pages, encoded to whatever that artifact is (a PNG for a social card). The
//! two cannot share a template for that reason, but everything around the
//! compile is the same whatever is being drawn, and lives here: the synthetic
//! module's file id, the tracked world, the dependency set, the diagnostics.
//!
//! An implementation supplies only what is specific to it: whether a page wants
//! one, where the file goes, the module to compile, and how to encode the
//! result. One `impl` plus one line in [`Sidecars::builtin`].
//!
//! The whole module is gated on the `sidecars` feature, which every artifact
//! that registers one enables: it is what pulls `typst-layout`, and a flavor
//! that draws nothing paged should not carry the layout engine.

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
///
/// The bindings are here because a sidecar that wraps the page in a template
/// wants exactly what a layout gets (`page.nav`, `page.strings`, the localized
/// date), and deriving them a second time is how the two drift apart. A sidecar
/// that draws something of its own shape, like a card, ignores them.
///
/// Assembled by [`Sidecars::draw`] rather than by the caller: without the paged
/// compile there is no implementation to read it, and a struct nothing reads is
/// a struct that goes stale.
#[cfg(feature = "sidecars")]
pub(in crate::engine) struct Cx<'a> {
    pub config: &'a Config,
    pub prepare: &'a Prepare<'a>,
}

/// One drawn sidecar file, ready to write.
///
/// Carries its own destination, so the writer needs to know nothing about which
/// artifact it is holding, and the kind that drew it, so the summary can still
/// name what it counts.
pub(in crate::engine) struct Artifact {
    /// The [`Sidecar`] that drew it.
    pub kind: &'static str,
    /// Where it lands under `dist`.
    pub path: PathBuf,
    pub bytes: Vec<u8>,
}

/// A kind of paged artifact a page can produce beside its HTML.
///
/// Object-safe and registered as a trait object, like the asset handlers and
/// the post-build processors: the set of sidecars is a list, and every pass
/// over it (drawing, writing, keeping) reads that one list.
#[cfg(feature = "sidecars")]
pub(in crate::engine) trait Sidecar: Sync {
    /// This kind's name, in one word. Used for three things that must agree:
    /// the synthetic module's file id, the label a compile error is reported
    /// against, and the noun the summary counts.
    fn name(&self) -> &'static str;

    /// Whether this page gets one. Read by the draw and by the prune, so a file
    /// an earlier build wrote is never swept out from under a cache hit.
    fn wanted(&self, config: &Config, page: &Page) -> bool;

    /// Where this page's artifact lands under `dist`.
    fn path(&self, config: &Config, page: &Page) -> PathBuf;

    /// The synthetic Typst module to compile: what binds this page to whatever
    /// template the artifact is drawn from.
    fn source(&self, cx: &Cx<'_>, page: &Page, rooted: &RootedPath) -> Result<String>;

    /// Encode the laid-out document into the bytes that get written.
    fn encode(&self, laid: &Laid, page: &Page) -> Result<Vec<u8>>;

    /// Lay this page's module out and encode it, reporting what the compile
    /// read: the template it imports and everything that template pulls in,
    /// which the page's own HTML compile never touches.
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

/// The registered sidecars. One list, read by everything that has to agree
/// about which files a page owns.
#[cfg(feature = "sidecars")]
pub(in crate::engine) struct Sidecars(Vec<Box<dyn Sidecar>>);

/// Without the paged compile there is nothing to register, and no
/// `typst-layout` to register it with.
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
    /// them.
    ///
    /// Derived from the page rather than from what was written, because a cache
    /// hit draws nothing and the file an earlier build left is still the page's:
    /// the prune reads this to keep it.
    pub(in crate::engine) fn planned(&self, config: &Config, page: &Page) -> Vec<PathBuf> {
        self.0
            .iter()
            .filter(|s| s.wanted(config, page))
            .map(|s| s.path(config, page))
            .collect()
    }
}

// The stubs below mirror the `sidecars`-on signatures exactly, so the caller
// compiles unchanged in both flavors. That is the whole point of them, and it is
// why they take a `self` they cannot use and return a `Result` they cannot fail.
#[cfg(not(feature = "sidecars"))]
#[allow(clippy::unused_self, clippy::unnecessary_wraps)]
impl Sidecars {
    pub(in crate::engine) fn builtin() -> Self {
        Self
    }

    /// Nothing is registered, so nothing is drawn and nothing is read.
    pub(in crate::engine) fn draw(
        &self,
        _project: &Project,
        _config: &Config,
        _prepare: &Prepare<'_>,
        _page: &Page,
    ) -> Result<(Vec<Artifact>, Deps)> {
        Ok((Vec::new(), Deps::default()))
    }

    /// Nothing is drawn, so this page owns no sidecar file.
    pub(in crate::engine) fn planned(&self, _config: &Config, _page: &Page) -> Vec<PathBuf> {
        Vec::new()
    }
}

#[cfg(all(test, feature = "sidecars"))]
mod tests {
    use super::*;

    /// A kind's name is what distinguishes its synthetic module's file id from
    /// every other kind's, so two sidecars sharing one would compile as the
    /// same `main` and the second would draw the first's document.
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
    /// were first seen. The name is the summary's noun, so a build that drew
    /// both counts them apart instead of reporting a total of two unlike things.
    pub kinds: Vec<(&'static str, usize)>,
    pub bytes: u64,
}

impl Tally {
    /// Count what this build drew. A cache hit draws nothing, so this counts
    /// only fresh artifacts; the files from earlier builds are still in `dist`
    /// and still kept.
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
