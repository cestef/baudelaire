//! Where in the project a DOM node was written.
//!
//! Typst carries a [`Span`] on every node it emits and the typed DOM hands it
//! straight through, so a rendered element can be traced back to the `.typ`
//! that produced it. Two passes need that: [`super::transform::spans`] stamps
//! `file:line:column` onto the markup for the live preview, and
//! [`super::lint`] anchors a finding at the exact bytes it is complaining
//! about. Both read one resolver, so a location can never be computed two
//! different ways.

use std::collections::HashMap;
use std::fmt;
use std::ops::Range;

use typst::syntax::{FileId, Source, Span, VirtualRoot};
use typst::{World, WorldExt};

use crate::world::PageWorld;

/// Where in the project's source something was written, as an editor counts it.
pub(super) struct Origin {
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

/// The bytes a node came from: the file, relative to the project root, and the
/// range within it. What a diagnostic needs in order to underline the source.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Site {
    /// Project-relative path, so the report reads `content/a.typ` and the
    /// checker can open it against the root it already knows.
    pub file: String,
    /// Byte offset of the node within that file.
    pub offset: usize,
    /// Byte length of the node's source.
    pub len: usize,
}

/// Resolves the compiler's spans to the places in the project they name.
///
/// Holds the sources it has looked up, because one page's elements come from a
/// handful of files (its body, its template, the modules those import) and a
/// walk asks about every element: without this, a page of a thousand nodes is a
/// thousand world lookups for the same few files.
pub(super) struct Origins<'a> {
    world: &'a PageWorld,
    sources: HashMap<FileId, Option<Source>>,
}

impl<'a> Origins<'a> {
    pub(super) fn new(world: &'a PageWorld) -> Self {
        Self {
            world,
            sources: HashMap::new(),
        }
    }

    /// The file and byte range `span` names, or `None` for anything the author
    /// cannot open: a detached span (an element this crate synthesized), or one
    /// in a package, whose paths name a download cache rather than the site.
    pub(super) fn site(&mut self, span: Span) -> Option<Site> {
        let (id, range) = self.bytes(span)?;
        Some(Site {
            file: id.vpath().get_without_slash().to_owned(),
            offset: range.start,
            len: range.len(),
        })
    }

    /// Where `span` was authored, as `file:line:column`.
    pub(super) fn locate(&mut self, span: Span) -> Option<Origin> {
        let (id, range) = self.bytes(span)?;
        let (line, column) = self.source(id)?.lines().byte_to_line_column(range.start)?;
        Some(Origin {
            file: id.vpath().get_without_slash().to_owned(),
            line: line + 1,
            column: column + 1,
        })
    }

    /// The shared half: which file a span belongs to, and the bytes it covers.
    fn bytes(&mut self, span: Span) -> Option<(FileId, Range<usize>)> {
        let id = span.id()?;
        if matches!(id.root(), VirtualRoot::Package(_)) {
            return None;
        }
        Some((id, self.world.range(span)?))
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
