#frontmatter((
  title: "Install",
  order: 1,
  tags: ("guide",),
))
#import "/templates/theme.typ": callout

Baudelaire is a single binary with no runtime dependencies. The Typst compiler
is built in, so there is nothing else to install.

== With cargo

```sh
cargo install baudelaire
```

This builds from source and drops the `baudelaire` binary in `~/.cargo/bin`.
Confirm it works:

```sh
baudelaire --version
```

#callout(kind: "note")[
  Fonts come from your system. On a slim server or CI image, install a font
  package such as `fonts-dejavu` so Typst has something to render with.
]

== From a release

Prebuilt binaries for Linux, macOS, and Windows are attached to every
#link("https://codeberg.org/cstef/baudelaire/releases")[release]. Download
the archive for your platform, extract it, and put the binary on your `PATH`.

Next: #link("quickstart.typ")[the quickstart].
