# Contributing to Baudelaire

Baudelaire is a single-binary static site generator where **the content and
templates are Typst, and Rust owns the engine**. There is no embedded scripting
language — the site's logic lives in Typst; the build's logic lives here.

## The pipeline

One line describes the whole build (`src/engine/mod.rs`):

```
discover → compile → render → write → process
```

- **discover / plan** (`src/content/`) — walk the content root into pages, then
  assemble the full page set: eligible content pages plus generated taxonomy and
  paginated index pages, with permalink uniqueness enforced. One entry point:
  `content::plan(config)`.
- **compile** (`src/engine/`, `src/world.rs`) — parse each page's (possibly
  template-wrapped) Typst and compile it to an HTML DOM, in parallel via rayon.
  A per-page dependency tracker records every file a page read, so the
  incremental cache (`src/graph/`) can invalidate precisely.
- **render** (`src/render/`) — per-page passes over the typed HTML DOM.
- **write** — every page's HTML, written in parallel.
- **process** (`src/engine/process.rs`) — whole-site passes that emit derived
  files (feeds, sitemap, robots, llms.txt, search index, redirects).

## The two extension points

Almost everything the build emits is one of two pluggable passes. Adding
either is **one `impl` plus one line in a registry** — nothing else in the
codebase learns about it.

| | `Processor` | `Transform` |
|---|---|---|
| where | `src/engine/process.rs` | `src/render/transform.rs` |
| scope | whole site, post-build | one page's DOM, mid-render |
| emits | derived files via `Emit` | mutates the `HtmlDocument` |
| registry | `Processors::builtin()` | `Transforms::builtin()` |
| gate | `enabled(&Config) -> bool` | `enabled(&Config) -> bool` |

Worked examples to copy: `src/engine/sitemap.rs` (a Processor),
`src/render/anchors.rs` (a Transform). `Emit` is trait-based, so a Processor is
unit-testable against the in-memory `Recorder` in `process.rs`.

## Conventions

See `AGENTS.md` for the full list. The load-bearing ones:

- **No orphan free functions.** Helpers live in `impl` blocks or private
  extension traits (`trait NodeExt`, `trait ElementExt`, `trait ValueExt`).
- **One-word names** where possible; method style (`foo.bar()`) over free
  functions.
- **HTML is never built with `format!`.** Use the typed DOM for render passes,
  or the escaping-correct `Xml` markup builder (`src/engine/xml.rs`) for
  build-output files.
- **Config parses through the dispatch tables** (`src/config/dispatch.rs`,
  `src/config/parse.rs`). Each scope's key list is the single source of truth for
  both parsing *and* the "did you mean?" suggestions — add a key there, not in a
  scattered match arm.
- **Errors are precise typed `Diagnostic`s** with `code`, `help`, and spans; no
  catch-all string error. Large variants are boxed.
- **One miette in the graph (7).** Dependencies that carry their own spans are
  lowered into it by hand rather than pulling in a second miette:
  - **kdl** is on the same miette 7, so a parse error passes straight through as
    a `diagnostic_source` (`src/error/config.rs`).
  - **Typst** reports through its own `SourceDiagnostic` (no `miette` dep),
    bridged in `src/error/typ.rs` and `src/error/content.rs`.
  - **wax**'s optional `miette` feature is pinned to miette 5, so enabling it
    would drag a second miette into the build. We leave it off and bridge its
    glob-error spans into an `Annotated` (miette 7) in `ContentError::bad_glob`.

## Tests

`cargo nextest run --workspace` is the runner. Every change ships tests:

- **Unit tests** (`#[cfg(test)] mod tests` per module) for pure logic — parse
  and merge, permalink building, frontmatter extraction, graph edges.
- **E2E tests** (`tests/`) build a fixture site end-to-end and assert the output
  HTML / file tree. Use `tempfile::TempDir`; never write into the repo. No
  real-time sleeps.

```sh
cargo nextest run --workspace --no-tests warn
cargo clippy --all-targets -- -D warnings
cargo run -- build            # build ./config.kdl
cargo run -- --profile dev serve
```
