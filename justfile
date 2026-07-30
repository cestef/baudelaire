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
ci: fmt clippy test (clippy SLIM) (test SLIM)

# Formatting is compile-free, so it runs first and fails fastest.
fmt:
    cargo fmt --all --check

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

# Build the docs site the way `.github/workflows/docs.yml` does.
[working-directory('docs')]
docs:
    cargo run -q -- build

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
