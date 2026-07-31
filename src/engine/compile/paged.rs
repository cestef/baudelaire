//! The compile every paged artifact runs: a synthetic module, laid out on
//! pages rather than exported as a DOM.
//!
//! Three things are drawn this way and none of them are alike: a social card is
//! one page of one document, a page's PDF is one document, a bundle is every
//! page of a collection at once. What they share is everything *around* the
//! compile, and it lives here so it is written once: the fabricated file id, the
//! tracked world that yields the dependency set, the diagnostics label, and the
//! PDF export options that have to be pinned or the bytes move on their own.
//!
//! [`super::sidecar`] runs this per page; [`super::bundle`] runs it per
//! document. Neither reimplements it.

use std::sync::Arc;

use typst::syntax::{FileId, RootedPath, Source, VirtualPath, VirtualRoot};
use typst_layout::PagedDocument;

use crate::error::{BaudelaireErrorKind, Result, TypstSourceDiagnostic};
use crate::graph::Deps;
use crate::world::{PageWorld, Project, Tracked};

/// One paged compile: the module text, and the label its diagnostics carry.
///
/// The label doubles as the suffix of the fabricated file id, so an artifact's
/// name is the one thing that distinguishes its compile from every other
/// compile of the same page.
pub(in crate::engine) struct Paged<'a> {
    /// What the fabricated module hangs off: a page's path for a per-page
    /// artifact, the bundle's id for a document. It has no file of its own, so
    /// this is also all a reader has to tell which compile failed.
    pub name: String,
    pub kind: &'a str,
    pub text: String,
}

/// A finished paged compile: the laid-out document, what the compile read, and
/// the world it read it through, which an exporter needs to report its own
/// failures with the same spans.
pub(in crate::engine) struct Laid {
    pub document: PagedDocument,
    pub deps: Deps,
    source: Source,
    world: PageWorld,
}

impl Paged<'_> {
    /// Lay the module out, reporting what it read.
    ///
    /// The dependency set matters as much as the document: nothing else in the
    /// build reads the paged template, so until the caller folds these in,
    /// nothing ties the artifact to the template that drew it.
    pub(in crate::engine) fn run(self, project: &Project) -> Result<Laid> {
        let source = Source::new(self.id(), self.text);
        let world = Tracked::new(project.world_for(&source));
        let compiled = typst::compile::<PagedDocument>(&world);
        let document = compiled
            .output
            .map_err(|errs| Laid::failed(errs, self.kind, &source, world.inner()))?;
        // `main` here is the fabricated id, not the page's, so a page source
        // this module imports its frontmatter from survives `dependencies`'
        // filter instead of being dropped as the compilation's own main.
        // Harmless: a page's text is already what its cache entry is keyed on,
        // and hashing it a second time as a dependency cannot disagree.
        let deps = project.dependencies(&world);
        Ok(Laid {
            document,
            deps,
            source,
            world: world.into_inner(),
        })
    }

    /// The module's file id: a project-root path suffixed with the artifact
    /// kind, so it collides with neither the page it is drawn from (which the
    /// module imports frontmatter from, and which must not be shadowed) nor
    /// with another kind's compile of the same page.
    fn id(&self) -> FileId {
        let name = format!("{}@{}", self.name, self.kind);
        let vpath = VirtualPath::new(&name)
            .expect("a page vpath with a suffix stays a valid relative path");
        FileId::new(RootedPath::new(VirtualRoot::Project, vpath))
    }

    /// The name a per-page artifact's module hangs off.
    pub(in crate::engine) fn of(rooted: &RootedPath) -> String {
        rooted.vpath().get_without_slash().to_string()
    }
}

impl Laid {
    /// Bridge typst's diagnostics against this compile's own source, for an
    /// exporter that fails the way the compiler does (PDF export rejects
    /// documents the layout accepted).
    pub(in crate::engine) fn failed(
        errs: typst::ecow::EcoVec<typst::diag::SourceDiagnostic>,
        kind: &str,
        source: &Source,
        world: &PageWorld,
    ) -> BaudelaireErrorKind {
        BaudelaireErrorKind::TypstCompile(TypstSourceDiagnostic::bridge(
            errs,
            (kind, source.text()),
            Arc::new(world.clone()),
        ))
    }

    /// Export this document as PDF, identified by `ident`.
    ///
    /// The options are pinned here for every caller, and this is the whole
    /// reason they are: both of typst's defaults are `Smart::Auto`, which
    /// stamps the instant of the export into the file, so two builds of an
    /// unchanged document produced two different files and every deploy
    /// re-uploaded the lot. The identifier is the artifact's own URL and the
    /// timestamp is the build's date, the one `sys.inputs.baudelaire.date`
    /// reports.
    #[cfg(feature = "pdf")]
    pub(in crate::engine) fn pdf(&self, kind: &str, ident: &str) -> crate::error::Result<Vec<u8>> {
        let options = typst_pdf::PdfOptions {
            ident: typst::foundations::Smart::Custom(ident.to_owned()),
            timestamp: self.world.stamp().map(typst_pdf::Timestamp::new_utc),
            ..Default::default()
        };
        typst_pdf::pdf(&self.document, &options)
            .map_err(|errs| Self::failed(errs, kind, &self.source, &self.world))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The module's id is the artifact's own: a sibling of whatever it is drawn
    /// from, suffixed with the kind. It must differ from the page's own id,
    /// which the module imports its frontmatter from and must not shadow, and
    /// from what another kind fabricates for the same page.
    #[test]
    fn the_module_id_is_the_name_suffixed_with_the_kind() {
        let rooted = RootedPath::new(
            VirtualRoot::Project,
            VirtualPath::new("content/a.typ").expect("a relative path"),
        );
        let id = |kind| {
            Paged {
                name: Paged::of(&rooted),
                kind,
                text: String::new(),
            }
            .id()
        };
        assert_eq!(id("card").vpath().get_without_slash(), "content/a.typ@card");
        assert_ne!(id("card"), id("pdf"), "two kinds, two compiles");
        assert_ne!(id("card"), FileId::new(rooted), "never the page itself");
    }
}
