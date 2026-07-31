//! Stamps each element with the source location it was compiled from.
//!
//! Typst carries a [`Span`] on every node it emits, and the typed DOM hands it
//! straight through, so a rendered element can say which `.typ` file, line, and
//! column produced it. With `html { spans #true }` each one gets a
//! `data-typst="file:line:column"`, which is what the live preview reads to
//! jump from something on screen back to the source that wrote it.
//!
//! Off in a published build: see [`crate::config::HtmlConfig::spans`].

use std::collections::HashMap;
use std::fmt;

use typst::syntax::{FileId, Source, Span, VirtualRoot};
use typst::{World, WorldExt};
use typst_html::{HtmlAttr, HtmlDocument};

use crate::config::Config;
use crate::world::PageWorld;

use super::{Cx, DocumentExt, ElementExt, Transform};

/// The attribute a stamped element carries. Short enough for
/// [`HtmlAttr::constant`]'s inline representation, so a name this file can
/// never spell wrongly at runtime.
const SOURCE_ATTR: HtmlAttr = HtmlAttr::constant("data-typst");

/// The [`Transform`] that stamps source locations onto the DOM.
pub(super) struct Spans;

impl Transform for Spans {
    fn enabled(&self, config: &Config) -> bool {
        config.html.spans
    }

    fn apply(&self, doc: &mut HtmlDocument, cx: &mut Cx<'_>) {
        let mut origins = Origins::new(cx.world);
        doc.walk(|element| {
            if let Some(origin) = origins.locate(element.span) {
                element.set(SOURCE_ATTR, &origin.to_string());
            }
        });
    }
}

/// Where in the project's source something was written.
struct Origin {
    /// The file, relative to the project root: what the author would open.
    file: String,
    /// One-based, as every editor counts them.
    line: usize,
    column: usize,
}

/// `file:line:column`, the spelling `+arg` takes on every editor command line.
impl fmt::Display for Origin {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}:{}", self.file, self.line, self.column)
    }
}

/// Resolves the compiler's spans to the places in the project they name.
///
/// Holds the sources it has looked up, because one page's elements come from a
/// handful of files (its body, its template, the modules those import) and the
/// walk asks about every element: without this, a page of a thousand nodes is a
/// thousand world lookups for the same few files.
struct Origins<'a> {
    world: &'a PageWorld,
    sources: HashMap<FileId, Option<Source>>,
}

impl<'a> Origins<'a> {
    fn new(world: &'a PageWorld) -> Self {
        Self {
            world,
            sources: HashMap::new(),
        }
    }

    /// Where `span` was authored, or `None` for anything the author cannot
    /// open: a detached span (an element this crate synthesized), or one in a
    /// package, whose paths name a download cache rather than the site.
    fn locate(&mut self, span: Span) -> Option<Origin> {
        let id = span.id()?;
        if matches!(id.root(), VirtualRoot::Package(_)) {
            return None;
        }
        let start = self.world.range(span)?.start;
        let (line, column) = self.source(id)?.lines().byte_to_line_column(start)?;
        Some(Origin {
            file: id.vpath().get_without_slash().to_owned(),
            line: line + 1,
            column: column + 1,
        })
    }

    /// The parsed source of `id`, looked up once per file. A file the world
    /// cannot produce is remembered as absent, so a missing one is not looked
    /// up again for every element that came from it.
    fn source(&mut self, id: FileId) -> Option<&Source> {
        let world = self.world;
        self.sources
            .entry(id)
            // Qualified: `PageWorld` has an inherent `source()` for its own
            // main file, which would shadow the trait method that takes an id.
            .or_insert_with(|| World::source(world, id).ok())
            .as_ref()
    }
}
