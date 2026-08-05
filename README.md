<h1 align="center">baudelaire</h1>

<p align="center">
    <img src="https://raw.githubusercontent.com/cestef/baudelaire/badges/crates-io.svg" alt="crates.io"/>
    <img src="https://raw.githubusercontent.com/cestef/baudelaire/badges/tests.svg" alt="tests"/>
    <img src="https://raw.githubusercontent.com/cestef/baudelaire/badges/coverage.svg" alt="coverage"/>
</p>

<p align="center">
    A static site generator where everything is <a href="https://typst.app">Typst</a>.
    Your pages, your layouts, and your data are one language, so a heading,
    a footnote, and a nav menu are all written the same way.
</p>

<p align="center">
    <b><a href="https://baudelaire.cstef.dev">Documentation</a></b> ·
    <a href="https://github.com/cestef/baudelaire/tree/main/themes">Themes</a> ·
    <a href="https://github.com/cestef/baudelaire/blob/main/CHANGELOG.md">Changelog</a>
</p>

## A page

Content is a `.typ` file. Frontmatter is an ordinary Typst binding, not a block
of YAML the compiler can't see:

```typ
#let frontmatter = (
  title: "Hello",
  date: datetime(year: 2026, month: 7, day: 30),
  tags: ("typst",),
  summary: "The first thing I published.",
)

= A heading

Some *content*, a #link("https://typst.app")[link], and anything else Typst can
set: math, figures, footnotes, tables.
```

Drop that in `content/posts/hello.typ` and it publishes at `/posts/hello/`,
joins the feeds and the sitemap, shows up under `/tags/typst/`, and gets
prev/next links from its neighbors. None of it is configuration.

Markdown works too, and its frontmatter is whatever its fence opens: `---` is
YAML, `+++` is TOML, `;;;` is KDL. A post copied out of another generator needs
no rewriting.

```md
---
title: Hello
tags: [typst]
---

Ordinary **prose**, and a [link](https://typst.app).
```

A `.md` page lowers to Typst before it compiles, so permalinks, taxonomies,
feeds, link checking and the incremental cache are the ones above, unchanged.

The layout that wraps it is Typst too, and builds real DOM instead of splicing
strings:

```typ
#import "@baudelaire/html:0.1.0": h

#let page(page, body) = {
  h("article", {
    h("h1", page.frontmatter.title)
    if page.date != none {
      h("time", datetime: page.date.iso, page.date.display)
    }
    body
  })
}
```

## Install

Prebuilt binaries for Linux (x86_64/aarch64), macOS, and Windows. Fetch the
script, read it, run it:

```sh
curl -fsSL https://baudelaire.cstef.dev/install.sh -o install.sh
less install.sh # read it
sh install.sh
```

On Windows, in PowerShell:

```powershell
irm https://baudelaire.cstef.dev/install.ps1 -OutFile install.ps1
notepad install.ps1     # read before you run
.\install.ps1
```

Or with Cargo:

```sh
cargo binstall baudelaire # prebuilt tarball, no compile
cargo install baudelaire  # build from crates.io
```

If you're feeling fancy:

```sh
cargo install --git https://github.com/cestef/baudelaire --branch main # build from git
```

Flags, slim builds, and what the installer does and does not verify:
[installation](https://baudelaire.cstef.dev/start/install/).

## Start

```sh
baudelaire init poem
# answer some questions...
cd poem
baudelaire serve
```

Your site is at [localhost:1821](http://localhost:1821), rebuilding as you save.

## Themes

A theme is templates, assets, and config defaults shipped as one unit, named
like any Typst dependency. Four come with the repository:

| | |
|---|---|
| [`albatros`](https://github.com/cestef/baudelaire/tree/main/themes/albatros) | A blog. Centered column, light and dark, tags, reading time. |
| [`spleen`](https://github.com/cestef/baudelaire/tree/main/themes/spleen) | A terminal. Monospace throughout, dark first, no JavaScript. |
| [`phares`](https://github.com/cestef/baudelaire/tree/main/themes/phares) | A manual. Sidebar from your content tree, search palette, on-page outline. |
| [`paysage`](https://github.com/cestef/baudelaire/tree/main/themes/paysage) | A portfolio. Landing page, project grid, one case study per project. |

They are inside the binary, so nothing is fetched. `baudelaire init` offers them
alongside the starter shapes, and an existing project adds one with:

```sh
baudelaire theme add albatros
```

Either way the files land in `themes/albatros/` and the config names them:

```kdl
theme "themes/albatros"
```

Everything a theme provides is a default: your file at the same path wins, and
your config wins key by key.

## License

Dual-licensed under [MIT](https://github.com/cestef/baudelaire/blob/main/LICENSES/MIT.txt)
or [Apache-2.0](https://github.com/cestef/baudelaire/blob/main/LICENSES/Apache-2.0.txt),
at your option.
