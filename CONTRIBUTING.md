# Contributing

Thanks for looking. Baudelaire is a single-maintainer project, so the most useful
thing you can do before writing code is open an issue and check the direction:
a design that does not fit is the expensive kind of rejected patch.

## Getting set up

Rust stable, at or above the `rust-version` in `Cargo.toml`. `rust-toolchain.toml`
pins `stable`, so `rustup` picks it up on its own.

```bash
git clone https://github.com/cestef/baudelaire
cd baudelaire
cargo build
cargo run -- --help
```

Optional, and worth having:

- [`just`](https://github.com/casey/just) to run the same checks CI does
- [`cargo-nextest`](https://nexte.st) for the test runner CI uses (`just test`
  falls back to `cargo test` without it)
- [`cargo-deny`](https://github.com/EmbarkStudios/cargo-deny) for `just audit`
- [`actionlint`](https://github.com/rhysd/actionlint) for `just workflows`

The e2e tests shell out to `curl`, `base64` and `git`. A missing one of those
looks like a product bug, so install them first.

### Editing the docs site

`docs/` is a real baudelaire site, so its templates import `@baudelaire/*` and
its scripts import `baudelaire:*`. Both are generated at build time, so an
editor resolves neither until they are written out:

```bash
just mirror
```

That writes `docs/.baudelaire/generated/` (gitignored, swept by `clean`). The
TypeScript half is already on the `include` list in `docs/tsconfig.json`; point
typst at the other half once, per project rather than per machine, since the
tables describe this site:

```jsonc
// tinymist
"tinymist.typstExtraArgs": ["--package-path", "<abs path>/docs/.baudelaire/generated/packages"]
```

`TYPST_PACKAGE_PATH` does the same for the typst CLI. Re-run `just mirror` after
changing a module in `src/world/module/` or `src/engine/asset/module/`.

## Before you push

```bash
just ci
```

That mirrors the `check` job: format, workflow lint, dependency audit, then
clippy and the tests over **both** feature flavors. The second flavor matters:
`--no-default-features` compiles a different set of modules, so a bare
`cargo clippy` cannot see a feature-off break.

Three things `just ci` does not cover, each its own CI job:

- `just docs` builds the docs site, which is itself a real baudelaire site.
  Run it after touching `docs/`, `themes/` or anything that changes output.
- `just msrv` compiles both flavors against the declared minimum Rust version.
- `windows` compiles both flavors on a Windows runner. It is compile-only: the
  e2e tests shell out to `base64`, which those runners do not have.

There is also a coverage ratchet on pull requests: a change that lowers coverage
fails. It instruments the test binary and not spawned subprocesses, so a
subprocess-based e2e test counts for almost nothing towards it. Prefer a unit
test where the behaviour allows one.

## House style

These are enforced, roughly in the order they get violated:

- **One source of truth.** Anything appearing twice (a key list, a name-to-enum
  map, a magic path, a default) collapses to one table or const that everything
  else derives from. `config/dispatch.rs` is the worked example: a config struct
  carries its own key table, and the "unknown key" error and its did-you-mean
  suggestions are generated from that same table, so they cannot drift from what
  actually parses.
- **Behaviour lives on types.** No orphan free functions.
- **Never use `Debug` or serde output as identity.** Fingerprints use a real
  `Hash` impl that destructures every field, so a new field fails to compile
  until it is handled.
- **Errors are precise typed miette diagnostics**, one class per type, with a
  stable `baudelaire::..` code. `error/fs.rs` is the model. Never widen an
  existing variant to reuse it, and never swallow one.
- **Diagnostic messages are marked up and their values escaped.** `` `code` ``
  and `*bold*` come from `ui/markup.rs`. A value interpolated into a message
  goes through `Code(.field)` or `Text(.field)`; a bare `{field}` inside a code
  span is a test failure, because a path containing a backtick would close the
  span and restyle the rest of the line.
- **Conversions are `From`/`Into`**, or Display-adapter newtypes (`Typst(&v)`,
  `Js(&v)`). Not `to_x()`/`from_x()`, and never an overloaded `Display` on a
  data type.
- **Config keys are one word.** A compound concept nests into a block:
  `drafts { suffix }`, never `draft-suffix`.
- **Generated output goes through its emitter**, not `format!`: Typst through
  `codegen::Value`, XML through `engine/emit/xml.rs`, JS through
  `engine/emit/script.rs`, HTML through the typed DOM.
- **Generated pages are opt-in or user-overridable**, never opinionated defaults.
- Prefer genericity, but a trait with one impl is a finding, not an abstraction.

Adding an emitter (`engine/emit/mod.rs`), a render transform
(`render/transform/mod.rs`), an asset handler (`engine/asset/handler.rs`), a
sidecar (`engine/compile/sidecar.rs`), a theme source (`theme/source.rs`) or a
virtual module (`engine/asset/module.rs` for JS, `world/module.rs` for Typst) is
one `impl` plus one line in that file's `builtin()`.

A CLI subcommand is the same shape but a different spelling, since `src/cli/` is
one module per subcommand: a module holding the clap args struct and its `Run`
impl, a variant on the `Command` enum, and an arm in `dispatch`.

### Settled, and not worth re-proposing

Each of these was built or seriously considered and decided against:

- An embedded scripting language (Rhai was tried).
- Menus in config (built, then reverted).
- Typst's typed-HTML API: it cannot express custom `data-*` attributes or SVG.
- Lossy image optimization. The pipeline is PNG-lossless by design.

## Commits

Conventional Commits, one line, no body:

```
feat(serve): a live status dot and a readable failure overlay
fix(html): footnotes render inside the layout
```

**One concern per commit, and each commit builds and passes on its own.** Work
naturally accumulates several concerns in one working copy; split before you
describe, not after.

`feat`, `fix`, `perf` and a breaking `refactor!` reach the changelog; `refactor`,
`docs`, `test`, `build`, `ci`, `chore` and `bench` are filtered out. A `!` marks
a breaking change.

## The changelog

**A user-facing change is not done until `CHANGELOG.md` says so.** `just changelog`
prints what the history claims since the last tag; the file is written by hand on
top of that, because a generated line says *what* changed and never *what to do
about it*.

Every breaking change wants its migration inline: the exact config line, flag
spelling or import that restores the old behaviour. Two things that are not
breaking still belong under `### Upgrading`: a `Renderer::SCHEMA` bump, which
costs everyone one cold rebuild, and any new check that can fail a build which
used to pass.

Do not run `just changelog-full`. It overwrites the file and destroys every
migration note in it.

## Reporting a bug

Include `baudelaire --version` (the long form, which lists the features this
build has), the config, and the smallest content tree that reproduces it. Run
with `-v` for per-page progress and debug logs, `-vv` for trace.

For anything with a security impact, see [SECURITY.md](SECURITY.md) instead:
report it privately rather than in an issue.
