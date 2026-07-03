# Research Notes - API assumptions, versions, gotchas

Pinned versions from local cargo registry (`cargo fetch` in temp project):
typst 0.15.0, typst-syntax 0.15.0, typst-html 0.15.0, typst-eval 0.15.0,
typst-library 0.15.0, typst-kit 0.15.0. kdl 4.7.1. notify 8.2.0.
anstream 1.0.0 (and 0.6.x - use 1.x). owo-colors 4.3.0. globset 0.4.18.
blake3 1.8.5. open 5.3.6.

## typst AST (`typst_syntax::ast`)

Typed view over CST. Root is `Markup`; `Markup::exprs()` → `impl Iterator<Item = Expr>`.

`Expr` variants that matter to us:
- `FuncCall` - `#frontmatter(...)`, `#html.frame[...]`, `image(...)`, `read(...)`. `FuncCall::callee() -> Expr`, `FuncCall::args() -> Args`.
- `Args::items() -> impl Iterator<Item = Arg>`. `Arg::Pos(Expr) | Named(Named) | Spread(Spread)`. `Named::name() -> Ident`, `Named::expr() -> Expr`.
- `ModuleImport` - `import "x.typ": a, b`. `ModuleImport::source() -> Expr` (usually `Expr::Str`). `bare_name()`, `new_name()`.
- `ModuleInclude` - `include "x.typ"`. `ModuleInclude::source() -> Expr`.
- `Link` - bare URL `https://...`. `Link::get() -> &EcoString`. (This is auto-linked URLs, NOT `#link(...)`. `#link` is a `FuncCall`.)
- `Ref` - `@target`. `Ref::target() -> &str`, `Ref::supplement() -> Option<ContentBlock>`.
- `Str` - `"..."`. `Str::get() -> EcoString` (unescaped).
- `Ident`, `FieldAccess` (`a.b`), `Closure`, `LetBinding`, etc.

`SyntaxNode`: `kind() -> SyntaxKind`, `children() -> impl Iterator`, `cast::<T: AstNode>() -> Option<T>`, `span() -> Span`, `range() -> Range<usize>` (via `LinkedNode`/`Source::range`).

Gotcha: `Markup::exprs()` filters newline-after-statement. For frontmatter scan we want top-level markup children; use `root().cast::<Markup>().unwrap().exprs()` and find first `Expr::FuncCall` whose callee is `Expr::Ident` with name `frontmatter`.

`Source::new(id, text)`, `Source::detached(text)`, `Source::root() -> &SyntaxNode`, `Source::text() -> &str`, `Source::id() -> FileId`. `Source` is cheap-clone (Arc<LazyHash>).

## typst eval (`typst_eval`)

`typst_eval::eval_string(world, library, sink, introspector, context, string, spans, mode, scope) -> SourceResult<Value>`.
- `mode = SyntaxMode::Code` parses as code block body → evaluates to last expr value.
- `scope: Scope` - pass `Scope::new()` populated with stdlib? No: pass empty `Scope`; the VM gets stdlib via `Scopes::new(Some(library))` internally. So datetime/string/array/dict literals resolve.
- For frontmatter dict: extract the `(...)` arg source text of `#frontmatter(dict-expr)`, call `eval_string(..., "(title: \"...\", date: datetime(...))", SpanMode::Uniform(detached), Code, Scope::new())` → `Value::Dict`.
- Needs `Tracked<dyn World>`, `TrackedMut<Sink>`, `Tracked<dyn Introspector>` (`EmptyIntrospector`), `Tracked<Context>`. Comemo tracking. Use `WorldExt`/`engine::Engine` construction pattern from `typst::compile`.

`typst_eval::eval` evaluates a full `Source` → `Module`. Not what we want for frontmatter (we want a single expr). Use `eval_string`.

## Frontmatter splice strategy

typst errors on unknown `#frontmatter(...)` at compile. Two options:
1. **Splice**: find the `FuncCall` node byte range via `Source::range(span)`, replace that range with a single newline in the source text, build a fresh `Source` for compile. Eval the dict arg text separately via `eval_string`.
2. **Register fn**: define `frontmatter` in the library scope as a native func that swallows args and stashes them. Harder - `Library::builder()` doesn't expose arbitrary scope mutation; would need custom `Module`/`Scope` plumbing.

Going with (1). Clean, no typst internals hacking. Byte range from `node.range()` (LinkedNode) or `world.range(span)`.

## typst-html DOM (`typst_html`)

`typst::compile::<HtmlDocument>(&world) -> Warned<SourceResult<HtmlDocument>>`.

`HtmlDocument`:
- `root() -> &HtmlElement`, `root_mut() -> &mut HtmlElement` (DOM mutation supported, documented caveat: can desync introspector - fine for our post-process since we re-serialize after).
- `root_node() -> &HtmlNode`, `info() -> &DocumentInfo`, `info_mut()`, `introspector() -> &Arc<HtmlIntrospector>`.
- `Output` impl: `HtmlDocument::create(engine, content, styles)`.

`HtmlNode`: `Tag(Tag)` | `Text(EcoString, Span)` | `Element(HtmlElement)` | `Frame(HtmlFrame)`. `HtmlNode::span()`, `HtmlNode::text(...)`.

`HtmlElement`: `tag: HtmlTag`, `attrs: HtmlAttrs`, `children: EcoVec<HtmlNode>`, `span`, etc. `HtmlElement::new(tag)`. Fields are public - mutate directly.

`HtmlAttrs(pub EcoVec<(HtmlAttr, EcoString)>)`: `push`, `push_front`, `get(attr) -> Option<&EcoString>`, `get_mut`. Direct tuple vec access.

`HtmlAttr`: constant consts in `typst_html::attr` (`href`, `src`, `class`, `id`, ...). `HtmlAttr::constant("...")`.

`tag` consts in `typst_html::tag` (`a`, `div`, `span`, `img`, ...). `HtmlTag` is a `PicoStr`-based newtype.

`typst_html::html(&doc, &HtmlOptions { pretty: bool }) -> SourceResult<String>` - serializes. `html_in_bundle(root, opts, link_resolver)` for sub-bundle.

`HtmlOptions::default()` → pretty false. Our `html.pretty` config maps to this.

This means: **no html5ever**. We walk `HtmlElement`/`HtmlNode` tree, mutate attrs/children, then serialize. Clean-URL href rewrite and embed (data URI) rewrite operate on this typed DOM (a `Transform` pass) before serializing. Text extraction for the search index runs on the serialized string — a deliberately lossy, tag-aware scan, not a structure-aware rewrite.

## typst World (`typst_library::World`)

`#[comemo::track] pub trait World: Send + Sync`:
- `library() -> &LazyHash<Library>`
- `book() -> &LazyHash<FontBook>`
- `main() -> FileId`
- `source(id) -> FileResult<Source>`
- `file(id) -> FileResult<Bytes>`
- `font(index) -> Option<Font>`
- `today(offset) -> Option<Datetime>`

`WorldExt::range(span) -> Option<Range<usize>>` (already used in our error code).

For multi-file project: `source(id)` must resolve any project file, not just main. Current `BaudelaireWorld` uses `SystemFiles` with `FsRoot::new(here)` - already handles arbitrary project paths via `FileId`/`RootedPath`. Need to confirm `main()` can be switched per-page during incremental build (rebuild set → compile each with its own main `FileId`). Either rebuild `BaudelaireWorld` per page (cheap-ish, fonts re-scan is the cost - cache the `FontStore`) or make `main` mutable.

`Library::builder().with_features(Features::from_iter([Feature::Html])).with_inputs(inputs).build()`. `Feature::Html` required for HTML export.

`FileId`, `RootedPath`, `VirtualPath`, `VirtualRoot::Project` - virtualize paths under project root. `VirtualPath::virtualize(here, canonical)`.

## kdl 4.7.1 (`kdl` crate)

Document-oriented (not serde). Preserves formatting. `KdlDocument::parse(s)` or `s.parse::<KdlDocument>()`.

- `doc.get("name") -> Option<&KdlNode>` (first child node by name)
- `doc.get_arg("name") -> Option<&KdlValue>` (first positional arg of first node named `name`)
- `doc.get_args("name") -> Vec<&KdlValue>` (all positional args)
- `doc.nodes() -> &[KdlNode]`, `doc.nodes_mut()`
- `doc.get_dash_vals("name")` - `- value` children convention
- `#[cfg(feature = "span")]` - `node.span() -> &SourceSpan` (miette-compatible). **Enable `span` feature for error spans.**

`KdlNode`: `name() -> &KdlIdentifier`, `entries() -> &[KdlEntry]`, `entries_mut()`, `get(key) -> Option<&KdlEntry>` (by index or name string), `children() -> Option<&KdlDocument>`, `children_mut()`, `ty() -> Option<&KdlIdentifier>` (type annotation `name<T>`), `span()` (feature).

`KdlEntry`: `name() -> Option<&KdlIdentifier>` (None = positional), `value() -> &KdlValue`, `value_mut()`, `span()` (feature).

`KdlValue`: `String(String)` | `RawString(String)` | `Base10(i64)` | `Base10Float(f64)` | `Base2/8/16(i64)` | `Bool(bool)` | `Null`. No datetime - store dates as strings, parse via `time` crate (already dep).

Parse strategy: manual `KdlDocument → Config` mapping in `config/parse.rs` (not serde). Gives us control over defaults, validation, error spans, shorthand conventions. Use `node.span()` for miette `#[label]`.

KDL features to enable in Cargo.toml: `kdl = { version = "4", features = ["span"] }`.

## notify 8.2.0

`RecommendedWatcher::new(handler, Config)` - event callback. Poll vs inotify backend auto. Debounce: `notify-debouncer-full` (separate crate, not fetched) or manual debounce. For serve mode, debounce ~100ms then trigger incremental rebuild. Watch `content/`, `templates/`, `config.kdl`.

Decision: use `notify-debouncer-full` for clean debounced events. Add to deps (not yet fetched).

## anstream 1.0.0 + owo-colors 4.3.0

`anstream::AutoStream` auto-detects color support (`NO_COLOR`, `CLICOLOR`, tty). Use `anstream::println`/`eprintln` wrappers or `StreamAutoStream::auto`. owo-colors for the actual color styles. Pattern:

```rust
use anstream::{AutoStream, ColorChoice};
use owo_colors::OwoColorize;
let stdout = AutoStream::auto(std::io::stdout().lock());
writeln!(stdout, "{} build complete", "◆".cyan().bold())?;
```

`ColorChoice::Auto` default; respect `NO_COLOR`. miette's `fancy` handler already respects this for diagnostics - our CLI chrome uses anstream.

## blake3 1.8.5

`blake3::hash(data) -> Hash`, `Hash::to_hex() -> String`. Content-hash per source file. Fast, no need for keyed hashing. Cache: `{ relpath: hex_hash }` JSON, compare to decide rebuild.

## globset 0.4.18

`GlobSetBuilder`, `Glob::new("posts/**/*.typ")`, `glob_set.is_match(path)`. Collection membership. Compile globs once at config load.

## open 5.3.6

`open::that(url)` - cross-platform browser launch for `serve.open`.

## Dependencies - corrected Cargo.toml additions

```toml
kdl = { version = "4", features = ["span"] }
notify = "8"
notify-debouncer-full = "0.5"        # debounced watcher for serve
anstream = "1"
owo-colors = "4"
globset = "0.4"
blake3 = "1"
open = "5"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
# html5ever NOT needed - typst-html typed DOM
# axum: reconsider - serve is tiny, evaluate tiny_http vs axum. Lean tiny_http (fewer deps) unless we need routing/middleware. Revisit at impl step 11.
```

Serve server decision deferred to step 11 - tiny_http likely wins on dep weight (single-binary philosophy). Note in PLAN.

## Open API questions to resolve at implementation

1. **`BaudelaireWorld` main switching**: confirm whether rebuilding `Arc<BaudelaireWorld>` per page (new `main` + `Source`) is cheap given memoized `comemo` caches. If fonts re-scan each rebuild, extract `FontStore` into shared `Arc` reused across pages. Test at step 5.
2. **Frontmatter arg extraction**: the dict arg is `Args::items()` first `Arg::Pos(Expr::Dict(_))` or `Arg::Pos(Expr::Parenthesized)` (dict literal wrapped). Get its source substring via `Source::text()[node.range()]`. Verify `node.range()` available on `SyntaxNode` (it's on `LinkedNode`; use `LinkedNode::new(root).find(span)` or `Source::range(span)`).
3. **Layout binding**: typst layout files export `let post(page, body) = {...}`. To compile a page *into* a layout, we need to eval the layout module, get the `post` function, call it with `(frontmatter, body_content)`. This is a second typst compile of the layout file + function call. Investigate `typst_eval::eval_closure` or eval the layout source as a module then call the exported fn in a wrapping source. Cleanest: generate a tiny wrapper source `#import "templates/post.typ": post; #post(frontmatter-args)[body-markup]` and compile that as main. Avoids string-templating HTML; only composes typst source - acceptable since typst *is* the renderer. Confirm at step 8.

All three flagged in PLAN implementation steps as "verify".
