#let frontmatter = (
  title: "Install",
  order: 1,
)
#import "/templates/theme.typ": callout

One binary, no runtime. The Typst compiler is inside it, so there is nothing
else to install.

```sh
curl -fsSL https://baudelaire.cstef.dev/install.sh -o install.sh
less install.sh          # read it first
sh install.sh
```

That picks the right prebuilt binary for your machine, verifies its checksum,
and drops it in `~/.local/bin`.

On Windows, `install.ps1` is the same script with the same knobs under the same
names. It installs to `%LOCALAPPDATA%\Programs\baudelaire`.

```powershell
irm https://baudelaire.cstef.dev/install.ps1 -OutFile install.ps1
notepad install.ps1     # read it first
.\install.ps1
```

Then:

```sh
baudelaire --version
```

Neither installer touches your `PATH`. If the install directory isn't on it,
both print the line that adds it.

== Installer options

Set them as environment variables. In PowerShell they're `$env:PREFIX` and so
on, or named parameters on a downloaded copy.

#table(
  columns: 3,
  align: (left, left, left),
  table.header([Variable], [Default], [Does]),
  [`PREFIX`], [`~/.local/bin`], [Where the binary lands.],
  [`VERSION`], [latest], [Pin a release, e.g. `v0.0.11`.],
  [`FLAVOR`], [`full`], [`slim` for the stripped build (below).],
  [`LIBC`], [auto], [`gnu` or `musl`. Linux only; PowerShell has no equivalent.],
)

```sh
PREFIX=/usr/local/bin VERSION=v0.0.11 sh install.sh
```

Prebuilt binaries exist for Linux (`x86_64` and `aarch64`, each `gnu` and
`musl`), macOS on Apple Silicon and Intel, and Windows on `x86_64`. Windows on
ARM has none: it would run the `x86_64` build under emulation, so the installer
refuses rather than quietly handing you a slow binary. Build a native one with
`cargo install baudelaire`.

#callout(kind: "note")[
  The checksum comes from the same origin as the tarball. It catches a truncated
  or corrupted download. It is not a signature.
]

The macOS builds are unsigned, so Gatekeeper blocks the first run. Clear the
quarantine flag once:

```sh
xattr -d com.apple.quarantine ~/.local/bin/baudelaire
```

== With cargo

```sh
cargo binstall baudelaire     # same prebuilt tarballs, no compile
cargo install baudelaire      # build from crates.io
cargo install --git https://github.com/cestef/baudelaire   # build from git
```

`cargo install` works on any platform Rust targets, which is the way in when no
prebuilt binary fits.

== Slim builds

Every release ships in two flavors. `full` is the default. `slim` drops the
optional capabilities and most of the binary size with them: the bundled fonts,
the CSS and JavaScript toolchains, the card renderer.

```sh
FLAVOR=slim sh install.sh
```

```powershell
$env:FLAVOR = "slim"; .\install.ps1
```

From source that's `cargo install baudelaire --no-default-features`, and you can
add any single capability back with `--features css`.

What `slim` leaves out:

#table(
  columns: 2,
  align: (left, left),
  table.header([Feature], [Off means]),
  [`embedded-fonts`], [Only fonts found on the host are available, so keep this
    one for containers and CI images that ship none.],
  [`js`], [`assets { bundle }` warns and `.js` files are copied verbatim.],
  [`css`], [`assets { minify }` warns and `.css` files are copied verbatim.],
  [`images`], [`assets { images { optimize } }` and `{ responsive }` warn, and
    PNG/JPEG assets are copied unchanged.],
  [`cards`], [`generate { cards }` warns and renders no card.],
  [`pdf`], [`generate { pdf }` warns and writes no PDF.],
  [`ssh`], [`deploy { ssh }` warns and that destination is skipped.
    `deploy { s3 }` is unaffected.],
  [`announce`], [`announce { standard }` warns, the `announce` command is gone,
    and the verification artifacts are not emitted.],
  [`themes`], [The `theme` command is gone, and the four shipped themes are not
    carried; a theme there is a directory you put in the project yourself.],
)

Turning one off never changes what a site that doesn't ask for it produces. A
site that does ask gets a `baudelaire::feature::missing` warning naming the
setting, the feature to rebuild with, and what was emitted instead.

#callout(kind: "warn")[
  `assets { fingerprint }` needs `css`. A verbatim stylesheet still names its
  images by their pre-hash filenames, so a slim build turns fingerprinting off
  for the whole build rather than shipping stylesheets that 404.
]

== From a checkout

Working on baudelaire itself:

```sh
just install
```

Next: #link("quickstart.typ")[the quickstart].
