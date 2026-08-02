#let frontmatter = (
  title: "Install",
  order: 1,
)

Baudelaire is a single binary with no runtime dependencies. The Typst compiler
is built in, so there is nothing else to install.

== Prebuilt binary

The quickest path, no Rust toolchain needed. Prebuilt binaries are published for
Linux (`x86_64` and `aarch64`, each in a `gnu` and a `musl` build), for macOS on
both Apple Silicon and Intel, and for Windows on `x86_64`. The installer picks
the right one, downloads the release archive, verifies its checksum, and drops
the binary in `~/.local/bin`.

It is deliberately readable, so fetch it, skim it, then run it:

```sh
curl -fsSL https://baudelaire.cstef.dev/install.sh -o install.sh
less install.sh          # read before you run
sh install.sh
```

On Windows, `install.ps1` is the same script in PowerShell: the same knobs under
the same names, the same steps, the same checksum promise. It installs to
`%LOCALAPPDATA%\Programs\baudelaire`.

```powershell
irm https://baudelaire.cstef.dev/install.ps1 -OutFile install.ps1
notepad install.ps1     # read before you run
.\install.ps1
```

Neither installer edits your `PATH`. When the install directory is not already
on it, both print the line that adds it.

The checksum it verifies is the `sha256` published alongside the tarball, from
the same origin as the tarball itself. That catches a corrupted or truncated
download; it is not a signature, and it is not evidence about who built the
release.

Set `PREFIX=` to install elsewhere, or `VERSION=vX.Y.Z` to pin a release. On
Linux the installer picks the `musl` build on a musl host and the `gnu` one
otherwise; `LIBC=musl` forces it. In PowerShell the same knobs are environment
variables: `$env:PREFIX`, `$env:VERSION`, `$env:FLAVOR`.

The macOS build is unsigned and unnotarized, so Gatekeeper blocks the first run.
Clear the quarantine flag once:

```sh
xattr -d com.apple.quarantine ~/.local/bin/baudelaire
```

Windows on ARM has no prebuilt binary. It would run the `x86_64` one under
emulation, so the installer refuses rather than quietly handing you a slow
build; `cargo install baudelaire` compiles a native one.

== With cargo

If you have a Rust toolchain, Cargo can install it three ways:

```sh
cargo binstall baudelaire     # prebuilt tarball, no compile
cargo install baudelaire      # build from crates.io
cargo install --git https://github.com/cestef/baudelaire   # build from git
```

`cargo binstall` pulls the same release tarballs as the installer script;
`cargo install` builds from source (slower, but works on any platform Rust
targets).

== Slim builds

Every release ships in two flavors. `full` is the default; `slim` drops the
optional capabilities, and with them the bulk of the binary: the bundled fonts
alone are ~9 MiB, the CSS and JavaScript toolchains ~3.2 and ~2.5 MiB, the card
renderer ~1 MiB.

```sh
FLAVOR=slim sh install.sh
```

```powershell
$env:FLAVOR = "slim"; .\install.ps1
```

Building it yourself is `cargo install baudelaire --no-default-features`, and
you can re-add any single capability with `--features css`.

What `slim` leaves out:

#table(
  columns: 2,
  align: (left, left),
  table.header([Feature], [Off means]),
  [`embedded-fonts`], [Text renders only with fonts found on the host, so keep
    it on for containers and CI images that ship none.],
  [`js`], [`assets { bundle }` warns and `.js` files are copied verbatim.],
  [`css`], [`assets { minify }` warns and `.css` files are copied verbatim.],
  [`images`], [`assets { images { optimize } }` and `{ responsive }` warn, and
    PNG/JPEG assets are copied verbatim. Lazy-loading attributes are markup
    only, so they stay.],
  [`cards`], [A `generate { cards { } }` block warns and produces nothing.],
  [`ssh`], [A `deploy { ssh { } }` block warns and that destination is skipped.
    `deploy { s3 { } }` is unaffected.],
  [`announce`], [An `announce { standard { } }` block warns, the `announce`
    command is unavailable, and the standard.site verification artifacts (the
    `.well-known` record and the per-page backlink) are not emitted.],
)

Each of these is a capability, not a behaviour switch: turning one off never
changes what a site that does not ask for it produces. A site that *does* ask
gets a `baudelaire::feature::missing` warning naming the setting, the feature to
rebuild with, and what was emitted instead. Nothing fails silently.

One capability is contagious. `assets { fingerprint }` needs the `css` feature,
because a verbatim stylesheet keeps naming its images by their pre-hash
filenames; hashing them anyway would leave the sheet pointing at files that no
longer exist. A slim build therefore turns fingerprinting off for the whole
build and says so, rather than emitting a site whose stylesheets 404.

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
