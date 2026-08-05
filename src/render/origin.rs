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
use std::path::Path;

use typst::syntax::{FileId, Source, Span, VirtualRoot};
use typst::{World, WorldExt};

use crate::content::{Page, Rebased};
use crate::world::{PageWorld, Wrapper};

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
    pub(super) fn site(&self, span: Span) -> Option<Site> {
        let (id, range) = self.bytes(span)?;
        Some(Site {
            file: id.vpath().get_without_slash().to_owned(),
            offset: range.start,
            len: range.len(),
        })
    }

    /// Whether `span` is part of `page`'s own content, rather than of the
    /// layout that wrapped it: the test for "did an author write this, or did a
    /// template?", and so the test the link graph draws its edges from.
    ///
    /// `dir` is the content tree, project-relative. Under it, a `.typ` file is
    /// content: a page's body reaches the compiler through `#include`, so its
    /// elements carry spans in the file its author typed. The extension is
    /// checked, not merely the directory, because a page's synthetic wrapper
    /// sits at the page's own path with a suffix (`content/a.typ@layout`) and
    /// would otherwise pass for content it is not.
    ///
    /// A lowered page is the exception, and has to be: its body is *inlined*
    /// into that wrapper, so every element it emits carries a span in
    /// `content/a.md@layout`, whose extension is the literal `md@layout`. The
    /// test rejected all of them, which is why a markdown page contributed
    /// nothing to the link graph at all: `page.backlinks` never named one, and
    /// the orphan report called a page unlinked that a markdown page linked to.
    pub(super) fn authored(&self, span: Span, dir: &Path, page: &Page) -> bool {
        // Straight off the span's own path, allocating nothing: this is asked of
        // every link a page carries, and [`Origins::site`] would build (and
        // throw away) a `String` per question.
        Self::file(span).is_some_and(|id| {
            let file = Path::new(id.vpath().get_without_slash());
            file.starts_with(dir)
                && (file.extension().is_some_and(|ext| ext == "typ") || self.inlined(id, page))
        })
    }

    /// Whether `id` is the wrapper `page`'s body was inlined into.
    ///
    /// Recognized as *the file being compiled* rather than by its name: the
    /// wrapper is this page's main source, which is the same thing
    /// [`Origins::mapped`] tests to decide whether a location needs
    /// translating, so the two cannot disagree about what a wrapper is. Only a
    /// lowered page qualifies; a `.typ` page's wrapper genuinely is chrome and
    /// stays out.
    #[cfg(feature = "markdown")]
    fn inlined(&self, id: FileId, page: &Page) -> bool {
        matches!(page.data, crate::content::Data::Lowered { .. }) && id == self.world.id()
    }

    /// No markdown dialect, no lowered page: nothing is ever inlined into a
    /// wrapper, so the only file that could be one is chrome. The signature
    /// mirrors the `markdown`-on form, so the caller reads the same either way.
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
    /// A lowered page compiles under a synthetic wrapper whose path names no
    /// file ([`Wrapper`]), so an untranslated location sends the preview's
    /// jump-to-source at `content/a.md@layout` and the editor opens nothing.
    /// Only the compiled page itself is translated: a span in a template
    /// already names a file its author wrote. A span the map cannot place is
    /// dropped, the same contract the diagnostic bridge keeps - the element
    /// goes unstamped rather than pointing at a line nobody wrote there.
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

    /// The shared half: which file a span belongs to, and the bytes it covers.
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
