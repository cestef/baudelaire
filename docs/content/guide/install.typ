#let frontmatter = (
  title: "Install",
  order: 1,
)

Baudelaire is a single binary with no runtime dependencies. The Typst compiler
is built in, so there is nothing else to install.

== Prebuilt binary

The quickest path, no Rust toolchain needed. Prebuilt binaries are published
for Linux `x86_64` and `aarch64`. The installer downloads the release tarball,
verifies its checksum, and drops `baudelaire` in `~/.local/bin`.

It is deliberately readable, so fetch it, skim it, then run it:

```sh
curl -fsSL https://baudelaire.cstef.dev/install.sh -o install.sh
less install.sh          # read before you run
sh install.sh
```

Set `PREFIX=` to install elsewhere, or `VERSION=vX.Y.Z` to pin a release.

== With cargo

If you have a Rust toolchain, Cargo can install it three ways:

```sh
cargo binstall baudelaire     # prebuilt tarball, no compile
cargo install baudelaire      # build from crates.io
cargo install --git https://codeberg.org/cstef/baudelaire   # build from git
```

`cargo binstall` pulls the same release tarballs as the installer script;
`cargo install` builds from source (slower, but works on any platform Rust
targets).

== From a checkout

Working on baudelaire itself? Build and install from the repo:

```sh
just install
```

== Confirm it works

```sh
baudelaire --version
```

Prefer it short? Add an alias in your shell: `alias bl=baudelaire`.

Next: #link("quickstart.typ")[the quickstart].
