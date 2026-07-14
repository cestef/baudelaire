# Baudelaire

A static site generator where everything is just [Typst](https://typst.app).

## Install

**Prebuilt binary** — Linux x86_64 / aarch64, no Rust toolchain. Fetch the
script, skim it, then run it:

```sh
curl -fsSL https://baudelaire.dev/install.sh -o install.sh
less install.sh          # read before you run
sh install.sh
```

It downloads the release tarball, verifies its `sha256`, and installs into
`~/.local/bin` (override with `PREFIX=`, pin a release with `VERSION=`).

**With Cargo:**

```sh
cargo binstall baudelaire     # prebuilt tarball, no compile
cargo install baudelaire      # build from crates.io
cargo install --git https://codeberg.org/cstef/baudelaire   # build from git
```

## Quickstart

```sh
baudelaire init poem
# answer some questions...
cd poem
baudelaire serve
```

See your new website at [localhost:1821](http://localhost:1821).

## License

Dual-licensed under [MIT](LICENSES/MIT.txt) or [Apache-2.0](LICENSES/Apache-2.0.txt), at your option.
