# Baudelaire - Implementation Plan

Typst-native static site generator. Single binary. Conventional defaults, deep customization via a typed processor pipeline. No string templating - typst renders, Rust builds.

## Design principles

1. **Typst is the renderer.** Pages are `.typ` files. HTML comes from `typst-html`. No Jinja/Tera/Handlebars. Layouts are typst files that call `html.frame`. Content-side logic lives in typst itself (a full language) — there is no second scripting layer.
2. **Processors extend the engine.** Post-build passes (feeds, sitemap, redirects, search, …) each implement a `Processor` trait and register in one table (`engine/process.rs`); a sibling `Transform` trait mutates the typed DOM. Adding an output = one impl + one line.
3. **Conventions over configuration, configuration over code.** Sensible defaults in `Default` impls. `config.kdl` overrides. Rust processors extend engine behavior itself.
4. **No string templating for URLs/HTML.** Permalinks are builders. HTML post-processing operates on parsed structure (via `html5ever` or typst's own HTML model), never regex on strings.
5. **Graph-driven incremental.** Content-hash per file. AST dep graph cascades invalidation. Cache is authoritative, not a hint.
6. **Clean errors.** Every user-facing failure is `#[derive(Error, Diagnostic)]` with `code`, `help`, spans. No string error soup.

## Architecture

```
cli        →  config  →  engine  →  graph  →  compile  →  render  →  process  →  write
              (KDL)              (AST)     (typst)     (builders)  (emitters)  (fs)
                ↑                  ↑          ↑           ↑           ↑
              defaults          edges     world      post-process  registry
```

### Crate layout

```
src/
  main.rs              clap entry, colored output init
  lib.rs               re-exports
  config/
    mod.rs             Config, Default, load(path), profile merge
    parse.rs           KDL → typed (via kdl crate)
    profile.rs         profile overlay logic
    defaults.rs        conventional Default values
  content/
    mod.rs             Page, Frontmatter, Collection, Taxonomy
    frontmatter.rs     #frontmatter(...) typst expr extraction + eval
    permalink.rs       Permalink builder (segment-based, not string fmt)
    collection.rs      discovery, convention+override hybrid
    taxonomy.rs        index page generation (per-tax index flag)
  graph/
    mod.rs             DepGraph
    scan.rs            AST edge extraction (import, include, link, asset, ref, layout)
    hash.rs            content-hash cache
    invalidation.rs    changed set + cascade
  engine/
    mod.rs             Engine: orchestrates pipeline, holds world+cache
    build.rs           full build
    serve.rs           dev server + watch
    check.rs           compile + linkcheck, no write
    process.rs         Processor trait + registry, Site view, Emit sink
    feed.rs            RSS/Atom emitter
    sitemap.rs         sitemap.xml emitter
    redirect.rs        redirect-stub emitter
    search.rs          search index emitter (json + inverted, optional client)
  render/
    mod.rs             Renderer trait + HtmlRenderer
    html.rs            typst-html output → post-process (structure-aware)
    layout.rs          layout binding (typst layout files)
  world.rs             (existing) extend for multi-file project
  serve/
    mod.rs             HTTP server, URL→page mapping, slash canonicalization
    watch.rs           notify-based watcher
  cli/
    mod.rs             command dispatch
    output.rs          colored reporting (anstream + owo-colors)
    progress.rs        build progress, step indicators
  error/
    mod.rs             (existing) extend
    config.rs          config parse errors
    graph.rs           dep graph errors
    serialize.rs       artifact (de)serialization errors
    render.rs          render errors
```

## Config (`config.kdl`)

### Final shape

```kdl
site   "Baudelaire"
url    "https://example.net"
lang   "en"
author "Claude"

content    "content"
dist       "public"
assets     "assets"
templates  "templates"

clean-urls   true
draft-suffix ".draft"
future       false              // build future-dated posts

inputs {
  site "https://example.net"
  env  "prod"
}

features "+html"

collections {
  // convention: top-level dir under content/ = collection, auto
  // override to re-glob, sort, permalink, add meta
  posts "posts/**/*.typ" sort="date" reverse=true permalink="/posts/{slug}/"
  notes "notes/**/*.typ" sort="order"
}

taxonomies {
  tags   kind="list" index=true
  series kind="tree" key="series" index=false
}

html {
  pretty true
  embed  "none"        // none | bundled | inline
}

cache {
  dir         ".cache"
  incremental true
}

serve {
  port  3000
  bind  "127.0.0.1"
  open  true
  watch true
}

search {
  formats "json" "inverted"   // index formats to emit (empty = off)
  fields  "title" "body" "tags"
  client  true                // also emit a tiny ES-module client per format
}

profiles {
  dev {
    url "http://localhost:3000"
    future true
    drafts true
    cache.incremental false
  }
  prod {
    html.pretty false
    html.embed  "bundled"
  }
}
```

### Config model

- `Config` struct with `#[derive(Default)]` giving conventional defaults (see `defaults.rs`).
- `Config::load(path)` → parse KDL → `Config::from_kdl(node)`.
- `Config::with_profile(self, name)` → deep-merge `profiles.<name>` overlay.
- `Config::with_cli(self, cli)` → CLI flags override (highest priority). Only flagged knobs: `out`, `port`/`bind`, `base-url`, `drafts`, `future`, `no-cache`, `profile`.
- Merge semantics: explicit value wins, `None`/absent = inherit. No silent defaults clobbering user intent.

### Conventional defaults (`defaults.rs`)

```rust
impl Default for Config {
    fn default() -> Self {
        Self {
            site: None,                       // warn if missing
            url: None,                        // warn if missing
            lang: "en".into(),
            author: None,
            content: "content".into(),
            dist: "public".into(),
            assets: "assets".into(),
            templates: "templates".into(),
            clean_urls: true,
            draft_suffix: ".draft".into(),
            future: false,
            inputs: Default::default(),
            features: vec!["html".into()],
            collections: Default::default(),  // convention fills
            taxonomies: Default::default(),
            html: HtmlConfig { pretty: true, embed: Embed::None },
            cache: CacheConfig { dir: ".cache".into(), incremental: true },
            serve: ServeConfig { port: 3000, bind: "127.0.0.1".into(), open: true, watch: true },
            profiles: Default::default(),
        }
    }
}
```

Convention fills collections at load time: every top-level dir under `content/` becomes a `Collection` with default `sort="order"`, `permalink="/{collection}/{slug}/"` unless overridden in `config.kdl`.

## Frontmatter

`#frontmatter(...)` typst expression at top of `.typ` file, before any other content.

```typst
#frontmatter(
  title: "Hello World",
  date: datetime(year: 2024, month: 1, day: 1),
  draft: false,
  slug: "hello-world",
  tags: ["intro", "typst"],
  template: "post.typ",
  order: 0,
  redirect: ["/old-path"],
)

#html.frame[
  ...page body...
]
```

### Extraction

1. Parse `.typ` via typst syntax (reuse `Source`).
2. Find first top-level markup node that is a function call to `frontmatter`.
3. Eval the args via typst's `eval` against an empty scope - frontmatter is pure data.
4. Map to typed `Frontmatter` struct. Unknown keys preserved in `extra: Dict` for template access.

Reserved keys (engine-meaningful): `title`, `date`, `draft`, `slug`, `taxonomies` (`tags`, `series`, ...), `template`, `order`, `redirect`. Everything else passes through to templates.

## Content model

```rust
struct Page {
    source: PathBuf,          // content/posts/hello.typ
    id: PageId,               // stable: collection + slug
    frontmatter: Frontmatter,
    body: Source,             // typst source sans frontmatter
    collection: CollectionId,
    permalink: Permalink,     // resolved
    output: PathBuf,          // dist path
    deps: Vec<Edge>,          // from AST scan
    hash: ContentHash,
}

struct Collection {
    id: CollectionId,
    glob: Glob,
    sort: SortKey,            // date | order | title
    reverse: bool,
    permalink: PermalinkTemplate,
    pages: Vec<PageId>,
}

struct Taxonomy {
    id: TaxonomyId,
    kind: TaxoKind,           // List | Tree
    key: String,              // frontmatter key to read
    index: bool,              // auto-generate index pages
    terms: HashMap<Term, Vec<PageId>>,
}
```

### Permalink builder

Not `format!("/{collection}/{slug}/")`. Segment-based:

```rust
struct Permalink(Vec<Segment>);

enum Segment {
    Literal(&'static str),
    Collection,
    Slug,
    Year, Month, Day,       // from frontmatter date
}

impl Permalink {
    fn render(&self, ctx: &PermalinkCtx) -> String { ... }
}
```

Parsed from config string `"/posts/{slug}/"` once into `Permalink`.

### Collection discovery (hybrid)

1. Scan `content/` top-level dirs → each becomes `Collection` with default config.
2. Merge `config.collections` overrides by id (dir name).
3. Globs resolve to `PageId`s.
4. Files at `content/` root (not in a dir) → `pages` collection (special, unsorted).

## Dependency graph

### Edges (`graph/scan.rs`)

AST walk over `Source` syntax tree. Edge kinds:

- `Import` - `#import "x.typ"`, `#include "x.typ"`
- `Link` - `#link("local")` targeting internal pages
- `Asset` - `image("local.png")`, `read("local.txt")`
- `Ref` - `@ref` cross-page references (custom convention: `@<collection>/<slug>`)
- `Layout` - frontmatter `template: "post.typ"` binding

Each edge → resolved `PageId` or asset path. Unresolved internal edge → `BrokenRef` error at build (unless `--strict-links false`).

### Invalidation (`graph/invalidation.rs`)

1. Hash every source file (content + templates + assets mtime).
2. Compare to `.cache/hashes.json`.
3. Changed set → walk dep graph reverse-edges → transitive closure = rebuild set.
4. Unchanged pages reuse cached HTML output.

Cache structure:
```
.cache/
  hashes.json          // { path: content_hash }
  outputs/             // { page_id: rendered_html }
  graph.json           // serialized DepGraph
```

## Engine pipeline

```
load_config
  → discover_content (convention + config)
  → build_graph (AST scan)
  → compute_rebuild_set (hash diff)
  → for each page in rebuild_set:
      compile(page)              // typst → HtmlDocument
      render(page)               // typst-html → HTML string (typed-DOM rewrite)
      layout(page)               // bind template, re-render if needed
      write(page)                // to dist/
  → generate_taxonomies          // index pages
  → generate_indexes             // _index.typ section pages
  → check_links                  // AST + post-HTML href scan
  → run_processors(site)         // feeds, sitemap, redirects, search — registry order
  → write_assets                 // passthrough
```

### Incremental vs full

- `--no-cache` or missing `.cache/` → full build, populate cache.
- Cache present → incremental. `clean` command wipes `.cache/` + `dist/`.

## Processors (`engine/process.rs`)

Extensibility is a typed Rust pipeline, not a scripting language — typst already
covers content-side logic. Two trait families, each with a single-source registry:

- **Emitters** (`Processor`) derive *new* files from the built site (feeds,
  sitemap, redirects, search). Read-only, run once at the end.
- **Transformers** (`Transform`, planned) *mutate* the typed DOM per page in the
  render path (minify, fingerprint, `html.embed`).

```rust
/// Read-only view of the built site handed to every processor.
struct Site<'a> {
    config:  &'a Config,
    pages:   &'a [Page],
    outputs: &'a [(&'a Page, &'a str)],   // page + rendered HTML
}

/// A sink for a processor's output (fs + reporting), abstracted for testing.
trait Emit {
    fn file(&mut self, path: &Path, contents: &str) -> Result<()>;
    fn note(&mut self, msg: fmt::Arguments) -> Result<()>;
    fn warn(&mut self, msg: fmt::Arguments) -> Result<()>;
}

/// One post-build pass over the site.
trait Processor {
    fn enabled(&self, config: &Config) -> bool { true }   // declarative gate
    fn run(&self, site: &Site, out: &mut dyn Emit) -> Result<()>;
}
```

`Processors::builtin()` lists the passes in run order — THE single source of what
runs post-build. Adding an output is one `impl Processor` + one line in that list;
each gates on its own config block (`feed`, `sitemap`, `search`, …). Content-side
customization (reading time, conditional rendering, per-page logic) is written in
typst, which sees page data directly.

## Render layer (`render/`)

### HtmlRenderer

1. `typst::compile::<HtmlDocument>` → `HtmlDocument`
2. `typst_html::html(&doc, &opts)` → HTML string
3. Post-process: operate on typst-html's own typed DOM (`HtmlDocument::root_mut()` → `HtmlElement`/`HtmlNode`/`HtmlAttrs`) before serializing. No external HTML parser. Operations:
   - Rewrite internal `href` to clean-URL form (walk `HtmlElement`, mutate `attrs` for `href`/`src`)
   - Bundle assets if `html.embed = bundled` (rewrite `src`/`href` to data URIs) — a `Transform`
4. Layout: if `template` set, re-compile page in scope where body is injected into layout's `html.frame`.

No `format!("<html>...</html>")` anywhere in Rust. All HTML comes from typst or the typed DOM mutation.

### Layout binding

Layout files live in `templates/`. A layout is a typst file exporting a function:

```typst
#let post(page, body) = {
  html.page(
    title: page.frontmatter.title,
    ...,
    body: html.frame(body),
  )
}
```

Engine compiles page body separately, passes to layout function, layout produces final `HtmlDocument`. Reuses typst's own module system - no custom templating engine.

## CLI (`cli/`)

### Commands

```
baudelaire [build]       build site (default)
baudelaire serve         dev server + watch
baudelaire check         compile + linkcheck, no write
baudelaire new <path>    scaffold content file (infer collection from dir)
baudelaire clean         rm dist + cache
baudelaire init          scaffold config.kdl + dirs
```

### Flags

```
global:
  -c, --config <path>     default config.kdl
  -r, --root <dir>        project root, default cwd
  -p, --profile <name>    dev | prod | custom
  -o, --out <dir>         override dist
      --base-url <url>    override url
      --port <n>          override serve.port
      --bind <addr>       override serve.bind
      --drafts            build drafts
      --future            build future-dated
      --no-cache          skip cache
      --strict-links      error on broken refs (default true, --strict-links false to warn)
  -v                      info
  -vv                     trace
  -q                      quiet
```

### Colored output (`cli/output.rs`)

`anstream` + `owo-colors`. Respects `NO_COLOR` / `CLICOLOR`. Palette:

- **title/milestone**: bold cyan
- **success**: green
- **error**: red (miette handles diagnostic rendering)
- **warning**: yellow
- **path**: blue underline
- **muted/secondary**: gray

Build report:
```
  ◆ build complete - 42 pages, 3 collections, 12ms
  ◇ cached - 38 unchanged
  ○ written - public/
```

Progress: one line per stage, spinner if tty, plain if piped.

## Serve mode (`serve/`)

- HTTP server (axum or tiny_http - pick lightweight).
- URL → page mapping: `/posts/hello` → `content/posts/hello.typ` output.
- Trailing slash: both work, canonical = no slash. `/posts/hello/` → 302 `/posts/hello`.
- Watch `content/` + `templates/` + `config.kdl` via `notify`. Change → incremental rebuild → live reload (inject `<script>` WebSocket or SSE).
- `serve.open` → `open` crate launches browser.

## Error model (extension)

New error kinds in `error/`:

- `config::ParseError` - KDL syntax, unknown key, bad type. `#[label]` on span.
- `config::MissingRequired` - e.g. `site` unset. `help("set `site \"...\"` in config.kdl")`.
- `graph::BrokenRef` - unresolved internal link/ref. `#[label]` on AST span, `help("did you mean ...?")`.
- `serialize::SerializeError` - artifact JSON (de)serialization failure, tagged by artifact.
- `render::LayoutError` - template binding failure.

All `#[derive(Error, Diagnostic)]`, `code(...)` per variant, `help(...)` actionable, `#[source]` chained.

## Implementation order

Tracer-bullet vertical slice first, then deepen.

1. **Config parse** - `config/` crate, KDL → `Config`, `Default`, profile merge, CLI overlay. Tests for merge semantics.
2. **CLI skeleton** - clap commands + flags, colored output init, `build`/`serve`/`check`/`new`/`clean`/`init` dispatch. `build` compiles single existing path end-to-end.
3. **Content discovery** - `content/` convention + override, `Page`/`Collection` structs, frontmatter extraction.
4. **Permalink builder** - segment-based, config + frontmatter slug resolution.
5. **Multi-file world** - extend `BaudelaireWorld` for project (root, multiple sources, templates dir).
6. **Dep graph + scan** - AST edge extraction, `DepGraph`, `hash.rs`, `invalidation.rs`.
7. **Engine build pipeline** - orchestrate load→compile→render→write, incremental via cache.
8. **Layout binding** - template function call pattern.
9. **Taxonomies + indexes** - auto-gen index pages, `_index.typ`.
10. **Processor pipeline** - `Processor` trait + registry, `Site` view, `Emit` sink; feeds/sitemap/redirects/search as emitters.
11. **Serve mode** - HTTP, URL mapping, watch, live reload.
12. **Link checking** - AST + post-HTML href scan, broken ref errors.
13. **`new` / `init` scaffolding** - template files, collection inference.
14. **HTML post-process** - typst-html typed DOM rewrite (clean URLs, embed) - no html5ever.
15. **Polish** - colored progress, error spans, docs sync.

Each step: `cargo nextest run --workspace` green + `cargo clippy --workspace --all-targets -- -D warnings` clean. Ship working end-to-end before next.

## Dependencies to add

```toml
kdl = "4"                    # config parsing
notify = "6"                 # file watching
# serve: tiny_http (lean, single-binary) vs axum - decide at step 11, leaning tiny_http
anstream = "0.6"             # colored output
owo-colors = "4"             # color primitives
open = "5"                   # browser launch
# html5ever NOT needed - typst-html provides typed DOM (HtmlDocument/HtmlElement/HtmlNode)
globset = "0.4"              # collection globs
serde = { version = "1", features = ["derive"] }   # cache serialization
serde_json = "1"             # cache files
blake3 = "1"                 # content hashing
```

## Non-goals (this phase)

- Themes/marketplace.
- Embedded scripting language — typst covers content logic; Rust processors cover the engine.
- Multi-language i18n.
- Image processing pipeline.
