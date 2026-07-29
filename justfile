# Match CI: workspace crates are strict (`-D warnings`); deps stay lenient
# because they're built with `--cap-lints allow`.
export RUSTFLAGS := "-D warnings"

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

# Build and install the binary from this checkout (contributors).
install:
    cargo install --path .
