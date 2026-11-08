# Tests and CI

**e2e tests shell out to `curl` and `base64`**, and `suite/incremental_e2e.rs` to
`git`. A missing host tool looks like a product bug.

**Every integration test is one binary, `tests/suite/`.** It was twenty-eight, and each
statically linked the whole typst tree at ~300 MB: linking them was 80% of what a build
cost and ten of the twelve gigabytes in `target/`. A file there is a module of
`tests/suite/main.rs`, which is also where a suite's feature gate lives
(`#[cfg(feature = "pdf")] mod pdf_e2e;`) rather than as a `#![cfg]` inside the file.
Two consequences: `common` is reached as `crate::common`, and **nextest is required**,
not preferred. It forks per test, so the isolation a separate binary used to give is
unchanged; under `cargo test` the whole suite would share one process, and the handful
of tests that set `HOME` or the working directory would race the rest.

**CI runs a coverage ratchet** (`cargo llvm-cov` over nextest). A PR that lowers
coverage fails. It is a separate job gated on `github.event_name != 'push'`, so it
never runs on a direct push to main. `LLVM_PROFILE_FILE` is inherited across exec, so
the `baudelaire` the e2e suite spawns is measured like any other code; under tarpaulin,
which this replaced, only the test binary was, and the whole CLI surface read as
uncovered.

`just ci` runs, in order: fmt, `actionlint` over the workflows, `cargo deny` over the
dependency tree, then clippy and nextest over both flavors. That covers the `lint` and
`test` jobs in `.github/workflows/ci.yml`. The workflow and audit recipes skip with a
note when their tool is missing, so a contributor without them still gets through; CI
installs both, and there they gate.

Four things `just ci` does not cover, each its own job: the coverage ratchet above;
the docs site build in `docs.yml` (`just docs`); `msrv`, which compiles both flavors
against `rust-version` from Cargo.toml (`just msrv`); and `windows`, which compiles
both flavors on a Windows runner. The cross-target matrix lives in `release.yml`, not
in CI.

**A CI job is one (runner, toolchain, feature set), and owns one target cache.** The
prefix names the job (`target-test-slim`, `target-msrv`, `target-windows-default`,
`target-coverage`), because RUSTFLAGS and the profile are part of every cargo
fingerprint and two jobs that differ in either share nothing. Three rules hold it
together, and breaking any one of them silently returns CI to cold builds:

- The key is `<prefix>-<os>-<arch>-<rustc>-<Cargo.lock>` with `<prefix>-<os>-<arch>-<rustc>-`
  as a fallback, so a lock bump recompiles the crates that moved rather than all 796.
  Without the fallback every `chore(release)` was a cold build, since the version bump
  touches baudelaire's own lock entry.
- Saves prune first: linked executables, `incremental/`, and the workspace's own rlibs
  all get rebuilt from source anyway, and they were 10 of the 12 GB. Pruned, the cache
  is ~500 MB and restores in seconds instead of two minutes.
- A job that only *reads* another job's cache (`docs.yml`, release's `gate`) must
  match its RUSTFLAGS and `CARGO_PROFILE_*` exactly, and must never save under that
  prefix.
- One job's oversized cache evicts every other job's, because the quota is the
  repository's and not the job's. The coverage tree is the one to watch: `cargo
  llvm-cov` writes a `.profraw` per instrumented *process*, and a suite that spawns the
  binary per test left 40 GB of them in a 47 GB tree. `cargo-cache` prunes those and
  takes a `max-size` ceiling, which is a guard on the neighbours rather than on the job
  holding it.
- An *empty* tree must never be written: a key is immutable, so every later run
  restores it as a hit and never replaces it, and the only cure is deleting the entry
  by hand. `cargo-cache` refuses that itself, which is what lets the saves stay
  `!cancelled()` rather than `success()`: a run that failed *after* building still
  caches the graph it built, and a red run is usually one failing test over a complete
  dependency graph.

**The `scope` job decides whether anything compiles at all**, from the event's own
diff. Its `case` names what *cannot* reach the compiler, never what can, so a directory
added to the tree later runs CI by default rather than silently escaping it; `src/`,
`tests/` and `benches/` are matched first so no prose pattern can claim a path under
them. It also carries the nightly's unchanged-SHA check, which used to gate only
coverage and now gates every compiling job.

**The Windows job is compile-only, and deliberately.** The e2e tests shell out to
`base64`, which Windows runners do not have, and the default feature set reaches
`aws-lc-sys` through `russh`. Both flavors run so the slim one, which drops `ssh` and
`aws-lc-sys` with it, reports separately from the full one.
