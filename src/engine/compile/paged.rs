//! The compile every paged artifact runs: a synthetic module laid out on pages
//! rather than exported as a DOM, with the file id, tracked world, diagnostics
//! label and export options they all share.

use std::sync::Arc;

use typst::syntax::{FileId, RootedPath, Source, VirtualPath, VirtualRoot};
use typst_layout::PagedDocument;

use crate::error::{BaudelaireErrorKind, Result, TypstSourceDiagnostic};
use crate::graph::Deps;
use crate::world::{PageWorld, Project, Tracked};

/// One paged compile: the module text, and the label its diagnostics carry.
pub(in crate::engine) struct Paged<'a> {
    /// What the fabricated module hangs off: a page's path for a per-page
    /// artifact, the bundle's id for a document.
    pub name: String,
    pub kind: &'a str,
    pub text: String,
}

/// A finished paged compile: the laid-out document, what the compile read, and
/// the world it read it through.
pub(in crate::engine) struct Laid {
    pub document: PagedDocument,
    pub deps: Deps,
    /// The text typst compiled, which is also what the injected values it read
    /// are recovered from.
    pub source: Source,
    world: PageWorld,
}

impl Paged<'_> {
    /// Lay the module out, reporting what it read: nothing else in the build
    /// reads the paged template, so only folding these deps in ties the
    /// artifact to the template that drew it.
    pub(in crate::engine) fn run(self, project: &Project) -> Result<Laid> {
        let source = Source::new(self.id(), self.text);
        let world = Tracked::new(project.world_for(&source));
        let compiled = typst::compile::<PagedDocument>(&world);
        let document = compiled
            .output
            .map_err(|errs| Laid::failed(errs, self.kind, &source, world.inner()))?;
        let deps = project.dependencies(&world);
        Ok(Laid {
            document,
            deps,
            source,
            world: world.into_inner(),
        })
    }

    /// The module's file id: a project-root path suffixed with the artifact
    /// kind, so it shadows neither the page it is drawn from nor another kind's
    /// compile of that page.
    fn id(&self) -> FileId {
        let name = format!("{}@{}", self.name, self.kind);
        let vpath = VirtualPath::new(&name)
            .expect("a page vpath with a suffix stays a valid relative path");
        FileId::new(RootedPath::new(VirtualRoot::Project, vpath))
    }

    /// The name a per-page artifact's module hangs off.
    pub(in crate::engine) fn of(rooted: &RootedPath) -> String {
        rooted.vpath().get_without_slash().to_owned()
    }
}

impl Laid {
    /// Bridge typst's diagnostics against this compile's own source, for an
    /// exporter that fails the way the compiler does.
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
            None,
        ))
    }

    /// Export this document as PDF, identified by `ident`.
    ///
    /// The identifier and timestamp are pinned because typst's `Smart::Auto`
    /// defaults stamp the instant of the export into the file, making two
    /// builds of an unchanged document two different files.
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
