# Match CI: workspace crates are strict (`-D warnings`); deps stay lenient
# because they're built with `--cap-lints allow`.
export RUSTFLAGS := "-D warnings"

# Replicate CI locally, cheapest checks first, failing fast.
ci: fmt clippy test

# Formatting is compile-free, so it runs first and fails fastest.
fmt:
    cargo fmt --all --check

# Clippy compiles the workspace; cheaper than running the tests, so it's next.
clippy:
    cargo clippy --workspace --all-targets -- -D warnings

# Prefer nextest (what CI runs); fall back to `cargo test` when it's absent.
test:
    #!/usr/bin/env sh
    if command -v cargo-nextest >/dev/null 2>&1; then
        cargo nextest run --workspace --no-tests warn
    else
        cargo test --workspace
    fi

# Install the binary & create an alias
install:
    cargo install --path .
    ln -sf ~/.cargo/baudelaire ~/.cargo/bl
