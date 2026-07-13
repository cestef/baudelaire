# Baudelaire refactor + feature plan

Derived from multi-agent audit (`.scratchpad/refactor-scout-reports.md`). Ordered by risk: bugs first, then mechanical refactors, then architecture, then features/UX/DX. Check off as landed.

## Phase 0 — correctness bugs (tiny, ship first) ✅
- [x] UX#7: `Report::status`/`event` raw `\r\x1b[2K` guarded behind `tty` field. (`src/cli/output.rs`)
- [x] 404 flat-emit: `destination` special-cases `404` → flat `404.html`. (`src/config/mod.rs`)
- [x] `config/value.rs` `kind` `"bool"` → `"boolean"`.
- [x] README quickstart `https://` → `http://`.

## Phase 1 — safe mechanical refactors ✅ (partial; convention-conflicting items skipped)
- [x] **H1** `redirect.rs` off `write!`-HTML: stub built via `Xml` builder (`fragment`/`doctype`/`text` added). `Escaped` escaper deleted; `ENTITIES` kept for decode.
- [x] **H3** collapsed `ScaffoldError`/`ServeError`/`ContentError` wrapper+kind into single derive enums. `ConfigError` **kept** — carries `text`/`span` + custom span/source/diagnostic_source logic, not a no-state wrapper.
- [x] **H4** folded `Sitemap` renderer into `SiteMap` processor. `Feed` **kept** — built once, `render` called per-format (a reused multi-method builder, not built-then-called-once).
- [~] **M1** SKIPPED — `Taxonomy`/`Pagination` unit-struct-as-namespace is the project's intentional convention (`Text`, `Env`, `Walk` same); AGENTS forbids orphan free fns. `EvalWorld.book` required by `World` trait impl, not dead.
- [x] `Deps::from_paths` → `impl From<Vec<PathBuf>>`.
- [~] LOW naming (`convention_id`/`bundle_slug`) SKIPPED — clear, necessary two-word names; rename risks collision/meaning loss. `by_collection`→`sections` deferred (marginal). `add_dependency`/`load_or_default`/`is_stale` etc. don't exist (audit named loosely).
- [~] `PageStatus::icon`→`&'static str` SKIPPED — would hardcode ANSI; owo-colors clearer, alloc trivial (verbose-only).

## Phase 2 — deeper refactors (genericity/duplication) ✅ (valuable items shipped; marginal ones skipped w/ reasons)
- [~] **H2** SKIPPED — hoisting section-builders to module consts trades one regularity for another, scatters doc comments, exposes `.fill()` at call sites. The `fn x { const X=Block(..); X.fill() }` pattern is clean; only the trait's *length* is the complaint.
- [x] **H5** `summary`→`Summary` Display newtype; `fingerprint`/`rfc3339` free fns → `Fingerprint`/`Rfc3339` trait methods. (`publication_uri`/`document_uri` kept — standard.site-specific, can't move onto the protocol-neutral `AtUri` without breaking layering.)
- [x] **M2** `BaseUrl::file(name)` for bare root filenames (robots). `by_collection`→`sections`.
- [x] **M3** `head` free fn → `ElementExt::head`; `walk{rewrite+srcset}` body → `ElementExt::assets(keys,f)` (fingerprint/embed unified).
- [x] **M4** atproto/client.rs: `Session::post` helper for put/delete; `read`/`field` free fns → `ResponseExt::json`/`ValueExt::field` traits.
- [x] **M5** (partial) boxed `ConfigErrorKind::Parse(Box<kdl::KdlError>)`. `Arc<str>`/`serve::Bind` deferred (marginal).
- [~] **M6** SKIPPED — `Listing` builder chain and `ValueExt` accessors are already clear; generic-closure collapse reduces readability.
- [~] **M7** SKIPPED — codebase deliberately uses module free-fn entry points (`content::discover`, `publish::run`); `configured`/`view` consistent with that, not orphans.

## Phase 3 — architecture (safe slices done; large rewrite deferred w/ reason)
- [x] **A1** (targeted) killed the `writes`/`outputs` double-chain — one `outputs` view now drives the parallel write, cache staging, and processors. (The audit's top concrete concern.)
- [~] **A1/A2/A3** full `Build` pass-struct + `Stage` pipeline + `Feature` enablement — DEFERRED. Behavior-preserving taste refactor of a correct, well-documented god-method; real regression risk with only E2E (no stage-level) tests. Better done under dedicated review.
- [x] **A4** extracted `content::plan(config)` (discover + taxonomy + pagination + uniqueness); engine calls it once. `ensure_unique` moved to `content`. Added `content/mod.rs` `//!` header (DX #8).
- [~] **A5** per-page render-cache invalidation — DEFERRED (big-site optimization, high effort).

## Phase 4 — features
- [x] **heading anchors** — `render/anchors.rs` `Transform`: every `<h1>`…`<h6>` gets a unique slug `id` (dedup `-2`/`-3`), author ids respected, gated `html { anchors #true }` (default on). Config field + defaults + parser + 2 e2e tests + docs.
- [x] **prev/next siblings** — each content page gets `page.nav.{prev,next}` (a `(url, title)` dict or `none`) for its neighbours within the collection, in the collection's sort order. `content::plan` assigns `Page.siblings` per collection over eligible pages (skips drafts/future, never crosses a boundary); the layout wrapper (`engine/layout.rs`) bakes them into `page.nav`, so a neighbour's add/remove/retitle refingerprints the sibling (cache-correct — regression test in `incremental_e2e`). 2 e2e tests, www pager on guide chapters + blog posts, `features/navigation.typ` doc.
- [x] **site sections in templates (single-source nav)** — every template gets `page.sections`: the site's content collections as an ordered array `(id, pages: ((url, title), …))`, each in its sort order, authored content only (generated listings excluded). `Engine::sections` builds it once per build, threaded into the wrapper alongside `nav` (via a new `layout::Context` struct grouping the template dict — replaced the 8-arg `Layout::new`). www sidebar is now generated from it (`theme.typ sidebar(sections)`) instead of a hand-kept list, so it can't drift from the pages or the pager. e2e `sections_expose_the_ordered_page_set_to_templates`; www features collection switched to `sort="order"` with curated `order` frontmatter.
- [~] TOC, analytics/head-snippet, reading-time, data files, per-page lang, related content, srcset — NOT done. TOC needs a placement design (marker element); analytics needs raw-HTML-in-typed-DOM handling; reading-time can't feed back into the template that produces the HTML. These want design decisions before implementation.

## Phase 5 — CLI/UX
- [x] **byte-size in build summary** — `Bytes(u64): Display` (1024-based). True total = page HTML + processed assets + generated files (threaded through `Emitter::bytes` + `asset::Processed`). Shows `· 344.8 KB`.
- [~] `--json`, OSC 8 hyperlinks, serve banner, `new`/`init` help, completions, spinner — NOT done (contained follow-ups).

## Phase 6 — DX / project hygiene
- [x] **CONTRIBUTING.md** — pipeline, two extension tables, conventions, test commands, miette-bridge note.
- [x] gitignore — already correct (`www/.gitignore` covers `.baudelaire/`+`public/`; nothing tracked). No action needed.
- [x] `content/mod.rs` `//!` header (done in A4).
- [~] `cargo fmt --check` in CI — SKIPPED: codebase uses a deliberately dense, rustfmt-divergent style; adding the check would mangle it or need a tuned `rustfmt.toml` (owner decision).
- [~] tests/common ureq/hermeticity, benches cold-compile — NOT done (follow-ups).
