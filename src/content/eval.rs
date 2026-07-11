use std::path::Path;

use typst::{
    Library, LibraryExt, World,
    comemo::Track,
    diag::FileError,
    engine::Sink,
    foundations::{Bytes, Context, Datetime, Dict, Scope, Value},
    introspection::EmptyIntrospector,
    syntax::{FileId, RangeMapper, RootedPath, Span, SyntaxMode, VirtualPath, VirtualRoot},
    utils::LazyHash,
};
use typst_eval::eval_string;
use typst_library::routines::SpanMode;

use crate::error::{ContentError, Result};

/// Minimal world for evaluating pure-data frontmatter. No file or font
/// access - frontmatter must be literal data.
pub(super) struct EvalWorld {
    library: std::sync::Arc<LazyHash<Library>>,
    book: std::sync::Arc<LazyHash<typst::text::FontBook>>,
}

impl EvalWorld {
    /// Evaluate a typst dict expression `(key: val, ...)` to a [`Dict`]. `path`
    /// names the source file in errors.
    pub(super) fn dict(src: &str, path: &Path) -> Result<Dict> {
        let world = Self::shared();
        let mut sink = Sink::new();
        // Map each evaluated node's span onto its own byte range in `src`, so a
        // syntax error underlines the exact offending text of the frontmatter
        // snippet (an identity map — `src` *is* the text we show as source). The
        // file id is cosmetic (labels resolve to ranges in `src` directly), so a
        // best-effort virtual path from the file name is enough.
        let vpath = path
            .file_name()
            .and_then(|name| VirtualPath::new(name.to_string_lossy()).ok())
            .unwrap_or_else(|| VirtualPath::new("frontmatter.typ").expect("a bare name is valid"));
        let id = FileId::new(RootedPath::new(VirtualRoot::Project, vpath));
        let mapper = RangeMapper::new(Some(0..src.len())).expect("identity range");
        let value = eval_string(
            Track::track(world),
            &world.library,
            Track::track_mut(&mut sink),
            EmptyIntrospector.track(),
            Context::none().track(),
            src,
            SpanMode::Mapped { id, mapper: &mapper, mapper_error_span: Span::detached() },
            SyntaxMode::Code,
            Scope::new(),
        )
        .map_err(|errs| ContentError::frontmatter_eval(path, src, errs))?;
        match value {
            Value::Dict(d) => Ok(d),
            other => Err(ContentError::frontmatter_not_dict(src, other).into()),
        }
    }

    /// The process-wide evaluator: the typst stdlib and font book are immutable
    /// and expensive to build, so they are constructed once, not per page.
    fn shared() -> &'static Self {
        static WORLD: std::sync::LazyLock<EvalWorld> = std::sync::LazyLock::new(|| EvalWorld {
            library: std::sync::Arc::new(LazyHash::new(Library::builder().build())),
            book: std::sync::Arc::new(LazyHash::new(typst::text::FontBook::new())),
        });
        &WORLD
    }

    fn path_from(id: FileId) -> std::path::PathBuf {
        id.vpath().get_without_slash().into()
    }
}

impl World for EvalWorld {
    fn library(&self) -> &LazyHash<Library> {
        &self.library
    }

    fn book(&self) -> &LazyHash<typst::text::FontBook> {
        &self.book
    }

    fn main(&self) -> FileId {
        typst::syntax::Source::detached("").id()
    }

    fn source(&self, id: FileId) -> typst::diag::FileResult<typst::syntax::Source> {
        Err(FileError::NotFound(Self::path_from(id)))
    }

    fn file(&self, id: FileId) -> typst::diag::FileResult<Bytes> {
        Err(FileError::NotFound(Self::path_from(id)))
    }

    fn font(&self, _index: usize) -> Option<typst::text::Font> {
        None
    }

    fn today(&self, _offset: Option<typst::foundations::Duration>) -> Option<Datetime> {
        None
    }
}
