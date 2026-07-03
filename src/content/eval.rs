use typst::{
    Library, LibraryExt, World,
    comemo::Track,
    diag::FileError,
    engine::Sink,
    foundations::{Bytes, Context, Datetime, Dict, Scope, Value},
    introspection::EmptyIntrospector,
    syntax::{FileId, Span, SyntaxMode},
    utils::LazyHash,
};
use typst_eval::eval_string;
use typst_library::routines::SpanMode;

use crate::error::{ContentError, Result};
/// Evaluate a typst dict expression `(key: val, ...)` to a [`Dict`].
pub fn dict(src: &str) -> Result<Dict> {
    let world = EvalWorld::new();
    let mut sink = Sink::new();
    let value = eval_string(
        Track::track(&world),
        &world.library,
        Track::track_mut(&mut sink),
        EmptyIntrospector.track(),
        Context::none().track(),
        src,
        SpanMode::Uniform(Span::detached()),
        SyntaxMode::Code,
        Scope::new(),
    )
    .map_err(|errs| ContentError::frontmatter_eval(src, errs))?;
    match value {
        Value::Dict(d) => Ok(d),
        other => Err(ContentError::frontmatter_not_dict(src, other).into()),
    }
}

/// Minimal world for evaluating pure-data frontmatter. No file or font
/// access - frontmatter must be literal data.
struct EvalWorld {
    library: std::sync::Arc<LazyHash<Library>>,
    book: std::sync::Arc<LazyHash<typst::text::FontBook>>,
}

impl EvalWorld {
    fn new() -> Self {
        Self {
            library: std::sync::Arc::new(LazyHash::new(Library::builder().build())),
            book: std::sync::Arc::new(LazyHash::new(typst::text::FontBook::new())),
        }
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
