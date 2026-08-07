//! `#md`: a chunk of markdown, rendered inside a Typst page.
//!
//! A native function rather than a Typst one, because lowering markdown is
//! Rust: it is the same [`Markdown`] pass a `.md` page goes through, so a
//! fragment embedded in a `.typ` page and a whole markdown page agree about
//! what a list, a table or a footnote becomes.
//!
//! Bound into the library's global scope under [`INTERNAL`] and re-exported as
//! `md` by `@baudelaire/markdown`, which is what supplies the site's own parser
//! settings. A virtual module's bindings are generated *source*, so a function
//! cannot be handed to one directly; the module closes over this instead.

use typst::World;
use typst::comemo::{Track, TrackedMut};
use typst::diag::{At, SourceResult, bail};
use typst::ecow::{EcoString, eco_format};
use typst::engine::Engine;
use typst::foundations::NativeElement;
use typst::foundations::{Args, Binding, Context, Func, NativeFunc, Scope, Value, func};
use typst::introspection::EmptyIntrospector;
use typst::routines::SpanMode;
use typst::syntax::{FileId, RootedPath, Span, Spanned, SyntaxMode, VirtualPath};
use typst::text::RawElem;

use crate::config::{Extension, MarkdownConfig, Named, RawHtml};
use crate::content::markdown::{Document, Markdown};

/// The name the raw function is bound under in the global scope.
///
/// Deliberately not a Typst identifier: one cannot open with `-`, so no page
/// can name this even by accident, and the only way to reach it is the string
/// lookup `@baudelaire/markdown` does. The spelling a site writes is `md`.
pub(super) const INTERNAL: &str = "--baudelaire-md";

/// Put [`md`] in `library`'s global scope, under [`INTERNAL`].
///
/// Bound by name rather than through `define_func`, which would take the
/// function's own: `md` is the spelling `@baudelaire/markdown` exports, and two
/// `md`s (one global and config-blind, one imported and config-aware) is one
/// too many.
///
/// `std` is rebound afterwards because [`Library::builder`] clones the global
/// scope into it at build time, so a definition added later reaches one and not
/// the other. `@baudelaire/markdown` looks this up *through* `std`, which is the
/// only way Typst source can ask whether a global exists, so leaving the two
/// disagreeing made the lookup miss a function that was right there.
pub(super) fn define(library: &mut typst::Library) {
    library
        .global
        .scope_mut()
        .define(INTERNAL, Func::from(<md as NativeFunc>::data()));
    library.std = Binding::detached(library.global.clone());
}

/// The markdown a caller wrote: a string as it stands, or the text of a raw
/// block.
///
/// A raw block is the spelling that keeps markdown *looking* like markdown in
/// an editor, since ```` ```md ```` highlights as markdown, and it is the only
/// literal form Typst leaves untouched. A content block is the trap: by the
/// time it arrives it is Typst markup, so it is refused by name rather than by
/// type.
fn markdown(source: Spanned<Value>) -> Result<String, EcoString> {
    match source.v {
        Value::Str(text) => Ok(text.into()),
        Value::Content(content) if content.func() == RawElem::ELEM => content
            .field_by_name("text")
            .and_then(|text| {
                text.cast::<EcoString>()
                    .map_err(|error| error.message().clone())
            })
            .map(Into::into),
        Value::Content(_) => Err(EcoString::from(
            "a content block is Typst markup by the time `md` sees it: \
             write the markdown as a string or a raw block",
        )),
        other => Err(eco_format!(
            "expected a string or a raw block, found {}",
            other.ty()
        )),
    }
}

/// The text of `path`, resolved the way a page's own `#include` would resolve
/// it: against the page being compiled, or against the project root when it
/// leads with `/`.
///
/// The page is [`World::main`], which for a baudelaire compile is the generated
/// wrapper sitting beside it, so its parent directory is the page's own.
fn read(engine: &Engine, path: &str) -> Result<String, EcoString> {
    let main = engine.world.main();
    let vpath = match path.starts_with('/') {
        true => VirtualPath::new(path),
        false => main
            .vpath()
            .parent()
            .ok_or_else(|| EcoString::from("the page has no directory to resolve against"))?
            .join(path),
    }
    .map_err(|error| eco_format!("{path} is not a usable path: {error}"))?;
    let id = FileId::new(RootedPath::new(main.root().clone(), vpath));
    let bytes = engine
        .world
        .file(id)
        .map_err(|error| eco_format!("could not read {path}: {error}"))?;
    String::from_utf8(bytes.to_vec())
        .map_err(|_| eco_format!("{path} is not valid UTF-8, so it is not markdown"))
}

/// Lower `source` as markdown and evaluate the Typst it becomes.
///
/// The settings arrive as plain values rather than as a config object because
/// they come *through Typst*: `@baudelaire/markdown` emits them as bindings the
/// way every virtual module emits its build data, and a native function is a
/// unit type that can capture nothing. Each is resolved through the same
/// [`Named`] table the config parses, so the two cannot drift.
#[func]
fn md(
    engine: &mut Engine,
    /// The markdown to render, taken straight off the argument list.
    ///
    /// Through [`Args`] rather than as a declared parameter for the span: an
    /// `Option<Spanned<T>>` parameter casts from a bare value and loses it, and
    /// the span is the whole point here. `@baudelaire/markdown` forwards the
    /// caller's arguments with `..`, which carries each one's span, so a
    /// mistake is underlined in the page that made it rather than in a package
    /// its author has never opened.
    ///
    /// Untyped, so a content block gets an answer rather than a type error:
    /// `md[**a**]` is the one mistake this function invites, and "Typst parsed
    /// it before I saw it" is not something a caller works out from "expected
    /// string, found content".
    args: &mut Args,
    /// The call itself, for a fault that belongs to no one argument.
    span: Span,
    /// The parser extensions to enable, by their config names.
    #[named]
    #[default]
    extensions: Vec<String>,
    /// What raw HTML in the markdown does: `refuse` or `drop`.
    #[named]
    #[default]
    html: Option<String>,
    /// Whether Typst written in the markdown is evaluated.
    #[named]
    #[default(true)]
    eval: bool,
) -> SourceResult<Value> {
    let source = args.eat::<Spanned<Value>>()?;
    // A file to read the markdown from instead, relative to the page, or rooted
    // at the project with a leading `/`.
    //
    // Read here rather than by `read()` in the module that calls this, because
    // a path inside a package resolves against *the package*: the module is
    // served from `@baudelaire/markdown`, so `read("intro.md")` there looks for
    // the file beside its own `lib.typ` and never beside the page. Going
    // through the world also makes it one of the page's tracked dependencies,
    // so editing it rebuilds exactly the pages that include it.
    let path = args.named::<Spanned<String>>("path")?;
    // Anything left over is a second piece of markdown, or a name this function
    // does not take, refused where it was written.
    args.clone().finish()?;
    let (text, span) = match (source, path) {
        (Some(source), None) => {
            let at = source.span;
            (markdown(source).at(at)?, at)
        }
        (None, Some(path)) => (read(engine, &path.v).at(path.span)?, path.span),
        // Both: the markdown is the argument to drop, so it is the one
        // underlined.
        (Some(source), Some(_)) => {
            bail!(source.span, "`md` renders markdown or a `path`, not both");
        }
        // Neither: no argument to point at, so the call itself. `..` forwarding
        // does not carry a callsite, which is why this one lands in the package
        // and the others do not.
        (None, None) => bail!(span, "`md` needs markdown to render, or a `path`"),
    };
    let config = MarkdownConfig {
        extensions: extensions
            .into_iter()
            .filter_map(|name| Extension::of(&name))
            .collect(),
        html: html
            .and_then(|name| RawHtml::of(&name))
            .unwrap_or(RawHtml::Refuse),
        eval,
        ..MarkdownConfig::default()
    };
    // `whole`, because a fragment is body from its first byte: a `---` at the
    // top of it is a thematic break, not a frontmatter fence belonging to a
    // page that has already been read.
    let document = Document::whole(&text);
    // The page this came from, for a diagnostic the lowering raises. Its own
    // spans are measured against `text`, which is not a file, so they are
    // dropped in favour of the one span that *is* real: the `#md` call.
    let path = span.id().map_or_else(
        || "<md>".to_owned(),
        |id| id.vpath().get_with_slash().to_owned(),
    );
    let (typst, _) = Markdown::new(&document, &text, &path, &config)
        .lower()
        .map_err(|error| error.to_string())
        .at(span)?;

    // Markup, not code: markdown lowers to a document body.
    //
    // `SpanMode::Uniform` points every span in it at the `#md` call, which is
    // the only position a reader can act on. The lowering keeps a source map
    // from generated Typst back to the markdown it came from, but that maps
    // into a *string* this call built, not into a file typst can quote, so
    // handing it through would name lines of markup nobody wrote.
    (engine.library.routines.eval_string)(
        engine.world,
        engine.library,
        TrackedMut::reborrow_mut(&mut engine.sink),
        EmptyIntrospector.track(),
        Context::none().track(),
        &typst,
        SpanMode::Uniform(span),
        SyntaxMode::Markup,
        Scope::new(),
    )
}
