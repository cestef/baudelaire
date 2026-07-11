# Refactor pass — findings & plan (2026-07-11)

Full-repo smell audit. Checklist ordered by phase. `[x]` = done.

## Phase 0 — cruft

- [x] Delete dead `BaudelaireError` wrapper + `SourceAlreadySet` (`src/error/mod.rs:29-104`, ~70 lines, zero external refs; `main.rs` converts `BaudelaireErrorKind` straight to `miette::Report`)
- [x] Remove unused deps: `tracing`, `tracing-subscriber`, `insta`; check `time/serde` feature
- [x] Delete `TaxoKind` — parsed, hashed, tested, never read (`src/config/mod.rs:183`); `kind=tree` behaves as `list`
- [x] Delete `Frontmatter::list` (zero callers)
- [x] Remove empty `src/serve/`, `src/script/` dirs
- [x] Remove leftover `site/` scaffold at repo root (init test artifact, own .git/.jj)
- [x] Rewrite `AGENTS.md` — still says "Rhai orchestrates", points at deleted `PLAN.md`/`docs/research.md`
- [x] Fix stale docs/comments: `search-index.json` → real filenames (`src/engine/search.rs:100`, `src/config/mod.rs:255`); `meta.rs:64` fingerprint claim vs `fingerprint.rs:34`; `transform.rs:31` Cx.assets consumers; `parse.rs:93` vs `error/config.rs:83` contradictory miette-version comments; `scaffold.rs:79` "post.typ" → layout.typ
- [x] `serve.rs:305` debouncer stored as `Box<dyn Any>` — type is nameable, use it

## Phase 1 — shared infrastructure

- [x] One DOM walker + URL-attr iterator in `transform.rs` — replaces 5 hand-rolled walkers (`rewrite.rs:59`, `image.rs:29`, `embed.rs:53`, `fingerprint.rs:44`, `meta.rs:44`)
- [x] `BaseUrl` type with `join(&Page)` — replaces 6 hand-joins (`feed.rs:56`, `sitemap.rs:46`, `robots.rs:30`, `llms.rs:25`, `meta.rs:123,132`) + one url-gating policy `Site::require_url(feature)` (feed/sitemap warn, llms/robots currently silent — unify to warn)
- [x] `crate::fs` gains `write_all` (mkdir-p+write; 3 copies at `engine/mod.rs:378`, `process.rs:113`, `asset.rs:167`), `canonicalize` (8 bypass sites; `Op::Canonicalize` exists unused), `exists`, `remove_file`
- [x] One MIME table (`embed.rs:100` vs `serve.rs:232` disagree; charset at serve call site)
- [x] `tests/common/mod.rs` harness — Sandbox/Tmp/Site copy-pasted 6× across e2e files
- [x] parking_lot Mutex (kills `.expect("lock")`: `world.rs:334+`, `serve.rs:282+`); flume over std mpsc in serve
- [x] Thread a `Root(PathBuf)` captured once, threaded to serve Filter/label + scaffold dir_name (chdir kept — it makes relative config paths resolve; the 3 current_dir() re-derivations are gone)
- [x] `NodeExt::int` → delegate to `ValueExt::integer`; single-value counterpart of `mapped()` for `sort()`/`taxo_kind`/png-`strip` hand-rolled enum matches
- [x] Shared `split_tail` (`render/asset.rs:31` vs `links.rs:69` verbatim dup)
- [x] `Page::title()`/`Item::of(&Page)` — title fallback ×3 (`pagination.rs:64`, `taxonomy.rs:93`, `llms.rs:33`); give taxonomy items `date`+`extra` like pagination
- [x] Single eligibility view (`skipped()` predicate dup + asymmetric: `pagination.rs:42` vs `engine/mod.rs:250`)
- [x] `eval.rs:16` free `dict()` rebuilds Library+FontBook per page → cached evaluator type
- [x] Compute page fingerprint/source once (`engine/mod.rs:262,286` double `source_for`); `Rendered` borrows page instead of clone
- [x] `Count::links` instead of hand pluralization (`engine/mod.rs:310`); sitemap date via `time` format (`sitemap.rs:58`)
- [x] `SiteMap::FILE` const shared with robots (`robots.rs:30` literal)
- [x] Interactivity flag computed once (`scaffold.rs:95` vs `257`)
- [x] One styling stack: `Input` (`prompt.rs:149`) raw print! → anstream; consider dropping console or isolating it
- [x] Slug precedence resolved once (`page.rs:55` then `permalink.rs:150` re-applies)
- [x] Move `impl Config::collection` from `content/page.rs:135` to config module
- [x] `Cargo.toml`: add `[lints]` table

## Phase 2 — silent-swallowing sweep

- [x] `boolean()` never errors (`parse.rs:109`): `clean "yes"` → false silently (verified). Error on non-bool, drop dead `default` param
- [x] i128 casts wrap (`parse.rs:119`, `value.rs:74`): `port 99999` → 34463. `try_into` + range errors
- [x] `Permalink::of` swallows parse errors (`permalink.rs:19`) — validate templates at config parse; `UnknownPlaceholder` diag currently unreachable. Also unterminated `{slug` accepted (`:48`); `expect("known placeholder")` panic (`:120`); `convention()` hand-assembles instead of parsing const template
- [x] Typed frontmatter errors (`frontmatter.rs:65`): `title: 3`→None, `draft:"yes"`→false, `date:"2024-01-01"`→dateless, all silent; no key did-you-mean
- [x] `load_config` (`cli/mod.rs:139`): every io error → "not found"; use facade, map only NotFound
- [x] `Engine::collect` (`engine/mod.rs:217`) keeps first error only — aggregate all page failures into related diagnostics
- [x] Frontmatter eval errors: no filename, no labels (`error/content.rs:126`); fix span resolution (mirror typ.rs), drop N+1 src clones; forward `kind.related()` (`content.rs:83`); hoist dup severity match (`content.rs:153` vs `typ.rs:70`)
- [x] Compile diagnostic line off-by-one (frontmatter strip adds a line; link spans correct — reuse that mapping)
- [x] Hooks cwd = project root, not `current_dir().unwrap_or_default()` (`hook.rs:61`)
- [x] Kill `io::Error::other` in `xml.rs:52` + delete context-free `Io` catch-all variant (`error/mod.rs:110`)
- [x] `feed.rs:134` `stamp` swallows format errors with `.ok()`
- [x] Watcher errors flattened away (`serve.rs:76`) — warn on Err arm
- [x] Reject: extra positional args (`dispatch.rs:57`), duplicate collection/taxonomy/profile ids (`config/mod.rs:62`), `..` in `destination()` (`config/mod.rs:97`), `paginate 0`/negative `.max(0)` clamps (`parse.rs:379,400,462`), empty `draft.suffix`, `features "-…"` (currently strips `-` and ENABLES)
- [x] `missing_profile` → dedicated kind listing valid profiles (`profile.rs:22`); profile error spans point into re-serialized text (`profile.rs:13`) — keep original source
- [x] `init` overwrite guard (`scaffold.rs:39` clobbers existing config/templates; `new_page` refuses — mirror it) + target preflight
- [x] Scaffold `{{}}` render: KDL-escape values, single-pass substitution (`scaffold.rs:325`)
- [x] `new posts/my-post` → resolve into content dir, append `.typ`, or error (writes literal cwd path today)
- [x] `git describe --tags --always` reports hash as tag (`world.rs:161`) — drop `--always`
- [x] `mapped()` accepts duplicates (`formats "rss" "rss"` = feed ×2); unify empty-list policy with `features`
- [x] Env `${TYPO}` → `""` silent (`value.rs:39`) — warn or error without `:-`
- [x] `prompt.rs`: zero-option panic (`:56`), EOF should fall back to default per module doc (`:86`)
- [x] Search JS: fetch failure → silent empty index (`engine.flat.js:12`, `engine.inverted.js:14`) — warn + error state

## Phase 3 — cache + output correctness

- [x] LinkMap in cache fingerprint (`cache.rs:84`) — slug edit leaves other pages' cached links stale, no warning (link check skips cached pages `engine/mod.rs:293`)
- [x] Embedded assets in dep set (`embed.rs:75`) — embed-on + fingerprint-off = stale data: URIs forever; also embed processed bytes, not source (minify/optimize bypassed)
- [x] Atomic blob/manifest writes + verify content-address on read (`cache.rs:119,148` — torn write = permanent corrupt hit); lock or grace-period prune for concurrent builds (`cache.rs:161`)
- [x] Dep paths serialized via `display()` (`cache.rs:129`) — lossy; serde PathBuf
- [ ] Collision detection pass over `(permalink, PageId, source)` — silent last-writer-wins today (verified); `posts/index.typ` + paginate = identical synthetic source (`listing.rs:134`) clobbering cache entries (`cache.rs:55`)
- [ ] Taxonomy slug collisions (`C++`/`C--` → `c`) + empty slugs → `/tags//` (`taxonomy.rs:108`)
- [ ] Unify slug policy: page slugs unslugified (spaces/emoji verbatim; sitemap `<loc>` unescaped, invalid) vs slugified terms; shared `Slug` type, percent-encode sitemap
- [ ] Nested content: `a/b/c/deep.typ` → `/a/deep/` — intermediate dirs dropped, cross-dir same-name pages overwrite. Decide: full nested permalinks or explicit error
- [ ] Fingerprint transform: cover `srcset`/`poster`; CSS `url()` rewrite through AssetMap in lightningcss visitor (`fingerprint.rs:41` — hard 404s today since original filename absent from dist)
- [ ] Profile overlay resets sibling fields (`parse.rs` builders start from `default()`; base `serve{bind}` + profile `serve{port}` resets bind) — fill-in-place like paths/typst/output
- [x] Taxonomy keys from config, not hardcoded `tags|series` (`frontmatter.rs:76` — custom key = zero pages silently)
- [ ] `--base-url` subpath: resolved links + asset refs ignore it (subdir deploy broken)
- [ ] `Js::bundle` writes only entry chunk — dynamic imports 404 (`asset.rs:285`)
- [ ] JPEG re-encode: honor EXIF orientation, skip when quality ≤ target (`asset.rs:140` — portraits rotate, generation loss)
- [ ] Redirect vs live permalink collision check (`redirect.rs:17` — stale redirect clobbers real page); add `noindex` meta to stubs (`redirect.rs:49`)
- [ ] Feeds: RSS untitled item needs title-or-description; Atom `<author>` (RFC 4287), `rel="self"` (`feed.rs:89,105`)
- [x] `data: "()"` for frontmatter-less pages — typst array, not dict `(:)` (`page.rs:50`); route through codegen
- [ ] `<title>` fallback (untemplated + default taxonomy pages have none; taxonomy defaults skip site layout entirely)
- [ ] og:image single owner: drop `content` arm from Fingerprint or Meta's resolve (`meta.rs:76` vs `fingerprint.rs:34`)
- [ ] Pagination: zero eligible members → emit empty page 1 (`/{collection}/` 404s today); count via `div_ceil` (`pagination.rs:53`)
- [ ] Root/index permalinks through `Permalink` type — 3 hand-`format!` sites, `index.typ` lands at `/posts/index/` while paginate makes `/posts/` (`page.rs:86`, `pagination.rs:95`)
- [ ] `codegen::Value::from_typst` via structural conversion, not `repr()` (`codegen.rs:73` — repr not round-trippable for non-data values)
- [x] `Call::find` accepts mid-document `#frontmatter` — restrict to leading (`frontmatter.rs:95`)
- [ ] `text.rs`: case-insensitive `</script>` close + skip-to-EOF when unclosed (`:44`); comment `-->` handling; numeric entities; char-boundary guard (`:68`)
- [ ] `links.rs`: percent-decode before join, case-insensitive ext, generic scheme detection (`:71`); LinkMap shouldn't drop canonicalize-failures/generated pages (`:31`)
- [ ] `asset.rs`: guard `dist`==`assets` before `remove_dir_all` (`:74`); lazy current_thread runtime (`:248` rebuilds multi-thread rt every rebuild); 3 `expect()` invariant panics → carry config in `Kind::Image`
- [ ] `hooks` extend-vs-replace inconsistency (`parse.rs:420`); block-presence enables robots/llms with no off switch (`:297` — accept `robots #false`)
- [ ] Feature-name validation at parse with span (`error/config.rs:31` spanless post-parse; share FEATURES table); spanless kinds shouldn't emit empty-source labels
- [ ] `layout.rs:46` `dir.display()` in import path — backslashes on Windows; take `&Value` for data (`:56`)
- [ ] Search: skip listing/taxonomy pages in corpus (`search.rs:67`); palette highlight over escaped HTML corrupts entities (`palette.js:23`); `doc.url` unescaped href (`:49`); inverted engine prefix-match to match flat UX

## Phase 4 — serve hardening

- [ ] Path traversal: percent-decode + reject resolves outside dist root (`serve.rs:218`, `GET /../../etc/passwd` escapes; `--bind 0.0.0.0` reachable)
- [ ] SSE: reap disconnected clients (one parked thread + sender leaked per connection, `serve.rs:287`); reap zombie children (`<defunct>` observed from `open` spawn)
- [ ] `read().unwrap_or_default()` serves empty 200 (`serve.rs:201`) — 404/500 on read failure
- [ ] Config reload: `.kdl` change rebuilds with stale startup config; root `config.kdl` not watched (`serve.rs:110`)
- [ ] Canonicalize root once for Filter (`serve.rs:339` — symlinked roots break strip_prefix, globs never match)
- [ ] 404 logging through session report (fresh `Report::stdout()` per 404 ignores `--quiet`, races `\r` line, `serve.rs:212`); `warn` during Silent rebuild garbles status line (`output.rs:122`); normalize level guards (`output.rs:162,174`)
- [ ] `is_content_change` drops `EventKind::Any` (`serve.rs:144`); debounce 500ms hardcoded → serve config key
- [ ] Serve rebuild-error overlay in browser (currently terminal-only, page silently doesn't reload); non-empty 404 body

## Features (post-refactor, ranked)

1. Template access to site data (list posts on homepage / related / custom indexes) — biggest gap
2. prev/next within collection for content pages
3. Custom 404 page
4. Feed quality: `<description>` from summary, per-collection/per-tag feeds
5. Page bundles / colocated assets (`content/posts/photo.png` silently ignored)
6. Stale dist pruning (deleted pages live until `clean`)
7. Reading time / word count in context
8. Head injection API (analytics/fonts/JSON-LD; title format)
9. External link + `#anchor` checking in `check`
10. Config upward discovery; skipped-drafts count; `-vv` phase timings

## Leave alone (audited good)

`process.rs` Processor/Site/Emit seam; Transforms registry; `dispatch.rs` tables; manual `Hash` on Config/BuildContext; `permalink.rs` PLACEHOLDERS; `codegen::Value` escape path; `fs.rs` facade shape; `xml.rs` escaping; hand-rolled base64 (RFC-verified); `Listing` abstraction; blob-store architecture; `error/typ.rs` span guards; incremental dep tracking (verified: data-file edit → exactly 1 page rebuilt; 34-page build ~200ms, warm 24ms).
