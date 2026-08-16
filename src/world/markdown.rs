//! `#md`: a chunk of markdown rendered inside a Typst page, through the same
//! [`Markdown`] pass a `.md` page goes through.

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

/// The name the raw function is bound under in the global scope, deliberately
/// not a Typst identifier, so no page can name it; a site writes `md`.
pub(super) const INTERNAL: &str = "--baudelaire-md";

/// Put [`md`] in `library`'s global scope, under [`INTERNAL`].
///
/// `std` is rebound afterwards because `Library::builder` clones the global
/// scope into it at build time, so a definition added later reaches one scope
/// and not the other.
pub(super) fn define(library: &mut typst::Library) {
    library
        .global
        .scope_mut()
        .define(INTERNAL, Func::from(<md as NativeFunc>::data()));
    library.std = Binding::detached(library.global.clone());
}

/// The markdown a caller wrote: a string as it stands, or the text of a raw
/// block, a content block having already been parsed as Typst markup.
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
fn read(engine: &Engine, path: &str) -> Result<String, EcoString> {
    let main = engine.world.main();
    let vpath = if path.starts_with('/') {
        VirtualPath::new(path)
    } else {
        main.vpath()
            .parent()
            .ok_or_else(|| EcoString::from("the page has no directory to resolve against"))?
            .join(path)
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
#[func]
fn md(
    engine: &mut Engine,
    /// The markdown to render, taken off the argument list rather than
    /// declared as a parameter, which would cast the value and lose its span.
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
    let path = args.named::<Spanned<String>>("path")?;
    args.clone().finish()?;
    let (text, span) = match (source, path) {
        (Some(source), None) => {
            let at = source.span;
            (markdown(source).at(at)?, at)
        }
        (None, Some(path)) => (read(engine, &path.v).at(path.span)?, path.span),
        (Some(source), Some(_)) => {
            bail!(source.span, "`md` renders markdown or a `path`, not both");
        }
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
    let document = Document::whole(&text);
    let path = span.id().map_or_else(
        || "<md>".to_owned(),
        |id| id.vpath().get_with_slash().to_owned(),
    );
    let (typst, _) = Markdown::new(&document, &text, &path, &config)
        .lower()
        .map_err(|error| error.to_string())
        .at(span)?;

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
