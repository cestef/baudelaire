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

Content is a `.typ` file. Its frontmatter is an ordinary Typst binding, not a
foreign block of YAML the compiler cannot see:

```typ
#let frontmatter = (
  title: "Hello",
  date: datetime(year: 2026, month: 7, day: 30),
  tags: ("typst",),
  summary: "The first thing I published.",
)

= A heading

Some *content*, a #link("https://typst.app")[link], and anything else Typst can
set: maths, figures, footnotes, tables.
```

Dropped in `content/posts/hello.typ`, that publishes to `/posts/hello/`, joins
the feeds and the sitemap, appears under `/tags/typst/`, and gets prev/next
links from its neighbours. Nothing above is configuration; it is the page.

The layout that wraps it is Typst too, and it builds real DOM rather than
splicing strings:

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

Prebuilt binaries for Linux (x86_64/aarch64) and macOS on Apple Silicon. Fetch
the script, skim it, then run it:

```sh
curl -fsSL https://baudelaire.cstef.dev/install.sh -o install.sh
less install.sh          # read before you run
sh install.sh
```

Or with Cargo:

```sh
cargo binstall baudelaire     # prebuilt tarball, no compile
cargo install baudelaire      # build from crates.io
cargo install --git https://github.com/cestef/baudelaire   # build from git
```

Flags, slim builds, and what the installer does and does not verify:
[installation](https://baudelaire.cstef.dev/guide/install/).

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
like any Typst dependency. Three come with the repository:

| | |
|---|---|
| [`albatros`](https://github.com/cestef/baudelaire/tree/main/themes/albatros) | A centred blog. System sans, light and dark, tags, reading time. |
| [`spleen`](https://github.com/cestef/baudelaire/tree/main/themes/spleen) | A terminal. Monospace throughout, dark first, no JavaScript. |
| [`voyage`](https://github.com/cestef/baudelaire/tree/main/themes/voyage) | A multilingual journal. Serif headings, language switcher, localized dates. |

Copy one into your project and name it:

```kdl
theme "themes/albatros"
```

Everything a theme provides is a default: your file at the same path wins, and
your config wins key by key.

## License

Dual-licensed under [MIT](https://github.com/cestef/baudelaire/blob/main/LICENSES/MIT.txt)
or [Apache-2.0](https://github.com/cestef/baudelaire/blob/main/LICENSES/Apache-2.0.txt),
at your option.
