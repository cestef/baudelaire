//! Where in the project a DOM node was written: the one resolver from a
//! [`Span`] to a file, a line and column, or a byte range.

use std::collections::HashMap;
use std::fmt;
use std::ops::Range;
use std::path::Path;

use typst::syntax::{FileId, Source, Span, VirtualRoot};
use typst::{World, WorldExt};

use crate::config::Config;
use crate::content::{Page, Rebased};
use crate::world::{PageWorld, Wrapper};

/// Where in the project's source something was written, as an editor counts it.
pub(super) struct Origin {
    /// Relative to the project root: what the author would open.
    file: String,
    /// One-based, as every editor counts them.
    line: usize,
    column: usize,
}

impl fmt::Display for Origin {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}:{}", self.file, self.line, self.column)
    }
}

/// The bytes a node came from: the file, relative to the project root, and the
/// range within it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Site {
    /// Project-relative path.
    pub file: String,
    /// Byte offset of the node within that file.
    pub offset: usize,
    /// Byte length of the node's source.
    pub len: usize,
}

/// Resolves the compiler's spans to the places in the project they name.
///
/// Holds the sources it has looked up, so a page of a thousand nodes is not a
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
    pub(super) fn site(&self, span: Span) -> Option<Site> {
        let (id, range) = self.bytes(span)?;
        Some(Site {
            file: id.vpath().get_without_slash().to_owned(),
            offset: range.start,
            len: range.len(),
        })
    }

    /// Whether `span` is part of `page`'s own content, rather than of the
    /// layout that wrapped it.
    ///
    /// `dir` is the content tree, project-relative; the extension is checked as
    /// well as the directory, because a page's synthetic wrapper sits at the
    /// page's own path with a suffix (`content/a.typ@layout`) and would
    /// otherwise pass for content it is not.
    pub(super) fn authored(&self, span: Span, dir: &Path, page: &Page) -> bool {
        Self::file(span).is_some_and(|id| {
            let file = Path::new(id.vpath().get_without_slash());
            file.starts_with(dir)
                && (Config::has_ext(file, Config::TYPST) || self.inlined(id, page))
        })
    }

    /// Whether `id` is the wrapper `page`'s body was inlined into, recognized
    /// as *the file being compiled* rather than by its name.
    #[cfg(feature = "markdown")]
    fn inlined(&self, id: FileId, page: &Page) -> bool {
        matches!(page.data, crate::content::Data::Lowered { .. }) && id == self.world.id()
    }

    /// Without a markdown dialect nothing is ever inlined into a wrapper.
    #[cfg(not(feature = "markdown"))]
    #[allow(clippy::unused_self)]
    fn inlined(&self, _id: FileId, _page: &Page) -> bool {
        false
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

    /// Where `span` was authored on a page this crate lowered, `map` being that
    /// page's second hop: from the Typst it was turned into back to the file
    /// its author typed.
    ///
    /// Only the compiled page itself is translated; a span the map cannot place
    /// is `None`, rather than a line nobody wrote there.
    pub(super) fn mapped(&mut self, span: Span, map: &Rebased) -> Option<Origin> {
        let (id, range) = self.bytes(span)?;
        if id != self.world.id() {
            return self.locate(span);
        }
        let (line, column) = map.map().position(map.locate(&range)?.start)?;
        Some(Origin {
            file: Wrapper::page(id.vpath().get_without_slash()).to_owned(),
            line,
            column,
        })
    }

    /// Which file a span belongs to, and the bytes it covers.
    fn bytes(&self, span: Span) -> Option<(FileId, Range<usize>)> {
        Some((Self::file(span)?, self.world.range(span)?))
    }

    /// The project file a span was written in, or `None` for anything the
    /// author cannot open: a detached span (an element this crate synthesized),
    /// or one in a package, whose paths name a download cache rather than the
    /// site.
    fn file(span: Span) -> Option<FileId> {
        let id = span.id()?;
        (!matches!(id.root(), VirtualRoot::Package(_))).then_some(id)
    }

    /// The parsed source of `id`, looked up once per file and remembered as
    /// absent when the world cannot produce it.
    fn source(&mut self, id: FileId) -> Option<&Source> {
        let world = self.world;
        self.sources
            .entry(id)
            .or_insert_with(|| World::source(world, id).ok())
            .as_ref()
    }
}
