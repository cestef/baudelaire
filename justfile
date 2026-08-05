# No `export RUSTFLAGS := "-D warnings"`. Setting RUSTFLAGS makes cargo discard
# `build.rustflags`/`target.<triple>.rustflags` from config, so `just` and a plain
# `cargo build` (or rust-analyzer) disagreed on rustflags, and two rustflag sets
# means two fingerprints: each alternation rebuilt all 875 crates. Strictness is
# unchanged where it matters, since `clippy` below already passes `-D warnings`.

# The released `slim` flavor: no embedded fonts, js, css, images or cards.
SLIM := "--no-default-features"

# CI lints and tests twice, once per release flavor, because
# `--no-default-features` compiles a different set of modules. The slim pass
# runs last: it is the rarer break, and the default set is what most edits hit.

# Replicate CI locally, cheapest checks first, failing fast.
#
# `audit` needs no toolchain and no compile, so it runs alongside `fmt` at the
# front rather than behind the two test passes.
ci: fmt workflows audit clippy test (clippy SLIM) (test SLIM)

# Formatting is compile-free, so it runs first and fails fastest.
fmt:
    cargo fmt --all --check

# Lint the workflows, including every `run:` block through shellcheck. Same
# skip-with-a-note rule as `audit`: CI installs the tool, a contributor need not.
workflows:
    #!/usr/bin/env sh
    if command -v actionlint >/dev/null 2>&1; then
        actionlint
    else
        echo "actionlint not installed, skipping workflow lint"
    fi

# Advisories, licenses, duplicate crates and source registries, per `deny.toml`.
# Skipped with a note rather than failing when cargo-deny is absent: CI installs
# it, and a contributor without it should still get through `just ci`.
audit:
    #!/usr/bin/env sh
    if command -v cargo-deny >/dev/null 2>&1; then
        cargo deny check
    else
        echo "cargo-deny not installed, skipping audit (cargo binstall cargo-deny)"
    fi

# Compile both flavors against the declared MSRV, what the `msrv` CI job does.
#
# Not a `ci` dependency: it is a second full compile of the tree on a second
# toolchain, which is a slow thing to put in front of every local run when the
# only edit that can break it is a new language feature.
#
# The version is read from Cargo.toml, and `RUSTUP_TOOLCHAIN` is what selects
# it: `rust-toolchain.toml` pins `stable`, and the environment variable is the
# one form that unambiguously outranks it.
msrv:
    #!/usr/bin/env sh
    set -eu
    version=$(sed -n 's/^rust-version = "\(.*\)"/\1/p' Cargo.toml)
    [ -n "$version" ] || { echo "Cargo.toml has no rust-version"; exit 1; }
    rustup toolchain install "$version" --profile minimal --no-self-update
    export RUSTUP_TOOLCHAIN="$version"
    cargo check --workspace --all-targets --locked
    cargo check --workspace --all-targets --locked --no-default-features

# Lint one flavor; compiles the workspace, but cheaper than running the tests.
clippy features="":
    cargo clippy --workspace --all-targets {{ features }} -- -D warnings

# Test one flavor with nextest (what CI runs), falling back to `cargo test`.
test features="":
    #!/usr/bin/env sh
    if command -v cargo-nextest >/dev/null 2>&1; then
        cargo nextest run --workspace {{ features }} --no-tests warn
    else
        cargo test --workspace {{ features }}
    fi

# The docs site is a real baudelaire site, so a typst error there fails its own
# workflow without failing `just ci`. Not a `ci` dependency: different workflow.
#
# Run from `docs/`, as the workflow does: `dist` is resolved against the working
# directory, not against `--root`, so building from the repo root would write the
# site to a stray `/public/` instead of to `docs/public`.

# Regenerate the checked-in config reference data from the dispatch tables.
#
# Writes `docs/generated/reference.typ`, a data module the docs' own reference
# page imports and renders. Committed rather than generated during the docs
# build: the docs site is a real baudelaire site, so its inputs have to be on
# disk before it builds, and making that build reach into this crate's internals
# would couple the two. The test below fails when the file and the tables
# disagree, so a stale copy cannot ship.
reference:
    BLESS=1 cargo nextest run --test reference the_checked_in

# Build the docs site from this checkout: the fast local loop, and what a docs
# edit should be checked with.
[working-directory('docs')]
docs: versions-list
    cargo run -q -- build

# Refresh the version picker's list: every release `CHANGELOG.md` declares that
# `git tag` confirms, newest first. Checked in so `baudelaire serve` works in
# `docs/` on a fresh clone, and rewritten here so the local loop never shows a
# version that has come or gone since.
versions-list:
    docs/versions.sh list > docs/generated/versions.csv

# Build the docs site the way `.github/workflows/docs.yml` deploys it: every
# published version, each rendered by its own released binary, the newest one at
# the site root. `docs/data/versions.csv` says which.
#
# Needs the network (a binary per version) and the tags fetched (a worktree per
# version), so it is slower than `just docs` by a lot. Follow it with
# `themes/demo/build.sh` directly rather than with `just previews`, which would
# rebuild the root and prune every versioned directory away.
versions:
    docs/versions.sh

# Mirror the generated modules for the docs site, so an editor resolves the
# `@baudelaire/*` imports its templates carry and the `baudelaire:*` imports its
# scripts carry.
#
# Not a `docs` dependency: a build already rewrites the TypeScript declarations,
# and the typst packages only move when this crate's modules do. Point tinymist
# at `docs/.baudelaire/generated/packages` once (see CONTRIBUTING).
[working-directory('docs')]
mirror:
    cargo run -q -- mirror

# A live demo site per shipped theme, into `docs/public/themes/<name>/`, so the
# docs site can link a real example of each.
#
# The loop lives in the script because the docs workflow runs it too, and CI has
# no `just`. Depends on `docs` and must run after it: the docs site sets
# `prune`, which deletes everything under its `dist` that its own build did not
# produce, previews included.
previews: docs
    themes/demo/build.sh

# Build and install the binary from this checkout (contributors).
install:
    cargo install --path .

# What the commit history says has changed since the last tag. Prints; the file
# is written by hand.
#
# A generated entry is a subject line, so it says what changed and never what to
# do about it. Read this, then write the Unreleased section of CHANGELOG.md:
# every breaking change wants a migration note, and a refactor usually wants no
# entry at all.
changelog:
    git-cliff --unreleased

# Regenerate CHANGELOG.md from scratch. Destroys every hand-written note in it;
# for bootstrapping or for repairing the structure, not for a release.
changelog-full:
    git-cliff -o CHANGELOG.md
