# Baudelaire 

<p align="center">
    <img src="https://raw.githubusercontent.com/cestef/baudelaire/badges/coverage.svg" alt="coverage"/>
    <img src="https://raw.githubusercontent.com/cestef/baudelaire/badges/tests.svg" alt="tests"/>
    <img src="https://raw.githubusercontent.com/cestef/baudelaire/badges/crates-io.svg" alt="crates.io"/>
    <br/>
    <img src="https://raw.githubusercontent.com/cestef/baudelaire/badges/binary-size.svg" alt="binary size"/>
    <img src="https://raw.githubusercontent.com/cestef/baudelaire/badges/binary-size-slim.svg" alt="binary size, slim"/>
    <img src="https://raw.githubusercontent.com/cestef/baudelaire/badges/typst.svg" alt="typst"/>
    <img src="https://raw.githubusercontent.com/cestef/baudelaire/badges/msrv.svg" alt="minimum supported rust version"/>
</p>

A static site generator where everything is just [Typst](https://typst.app).

## Install

**Prebuilt binary**: Linux x86_64 / aarch64, no Rust toolchain. Fetch the
script, skim it, then run it:

```sh
curl -fsSL https://baudelaire.cstef.dev/install.sh -o install.sh
less install.sh          # read before you run
sh install.sh
```

It downloads the release tarball, checks it against the `sha256` published
alongside it, and installs into `~/.local/bin` (override with `PREFIX=`, pin a
release with `VERSION=`). The checksum comes from the same origin as the
tarball, so it catches a corrupted or truncated download; it is not a signature.

**With Cargo:**

```sh
cargo binstall baudelaire     # prebuilt tarball, no compile
cargo install baudelaire      # build from crates.io
cargo install --git https://github.com/cestef/baudelaire   # build from git
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
