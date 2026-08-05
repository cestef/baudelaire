#let frontmatter = (
  title: "Markdown pages",
  order: 3,
)
#import "/templates/theme.typ": callout

A `.md` file under `content/` is a page, exactly like a `.typ` one. Its
frontmatter is a `---` block of #link("../configure/overview.typ")[KDL], the
same language `config.kdl` is written in.

```md
---
title "Install"
order 1
tags "cli" "setup"
---

# Install

One binary, **no runtime**.
```

Markdown is a source dialect, not a second pipeline: a page lowers to Typst
before it compiles. Permalinks, collections, taxonomies, link checking,
highlighting, feeds, search and the incremental cache are the ones that were
already there, because by the time the build looks at the page there is nothing
markdown-shaped left.

== Frontmatter

KDL has no `key: value` line. A node's *shape* is its type, and the four shapes
are the ones KDL itself distinguishes:

#table(
  columns: 2,
  align: (left, left),
  table.header([Written], [Means]),
  [`draft`], [`true`. A bare node is a flag.],
  [`title "A"`], [The string `"A"`.],
  [`tags "rust" "typst"`], [A list of two.],
  [`author { name "cstef" }`], [A dict. `key=value` entries join it.],
)

There is no way to write a one-element list, because one argument is always the
scalar. That is the counterpart of Typst's trailing comma in `("rust",)`.

Every #link("frontmatter.typ")[frontmatter key] behaves as it does on a Typst
page: the same built-ins, the same collection schema, the same typo suggester.
Pasted YAML is caught by that last one, since `title:` is a valid KDL node name
and simply is not a key any page has.

Dates are the one place the shapes are not enough, since KDL has no date
literal. Write the ISO day:

```md
---
title "The night train"
date "2026-08-05"
---
```

== What is accepted

CommonMark, plus the GFM set. Which extensions exist, and which are on without
asking, is in the #link("../configure/reference.typ")[reference] under
`content.markdown.extensions`; a `*` marks the ones you already have.

#callout(kind: "note")[
  A heading renders one level down, so `#` becomes `<h2>`. That is not markdown
  being mistreated: Typst's own `=` does the same, and a `.md` page and a `.typ`
  page with one outline should produce one document. Six `#` clamp to five,
  because there is no seventh heading for them to be.
]

Raw HTML is refused by default. The DOM a build produces is typed, so a string
of markup has nowhere to be spliced into it; a comment is the exception, since
it has nothing to lose. Write the element instead, in a fence that runs (below),
or let the site drop it (`html "drop"`, further down).

== Fenced blocks

A fence's info string is a language, then space-separated `key` or `key=value`
parameters:

````md
```kdl
site "Example"
```

```typ eval
#callout(kind: "note")[Rendered by the template's own helper.]
```
````

#table(
  columns: 3,
  align: (left, left, left),
  table.header([Parameter], [Default], [Does]),
  [`eval`], [off], [Run the block as Typst instead of showing it. `typ` only.],
)

A fence is a sample by default, including a Typst one, so a page can document
Typst without running it. `eval` is how a markdown page reaches a template
helper, a chart, or an element markdown has no syntax for:

````md
```typ eval
#html.elem("aside", "Something markdown cannot say.")
```
````

#callout(kind: "warning")[
  An error inside an `eval` fence is reported against the Typst the page lowered
  to, not against the line you wrote. Keep what runs short.
]

== A worked example

This site's own #link("../lookup/changelog.md")[changelog] is a markdown page,
generated from the repository's `CHANGELOG.md` with a frontmatter block in front
of it. It exercises tables, reference links, nested lists and sixty-odd fenced
samples, and nothing about it was rewritten by hand.

== Linking to one

A link names the file, so a markdown page is reached by its `.md` name. The link
checker resolves and verifies it exactly as it does a `.typ` one.

```md
[the changelog](../lookup/changelog.md)
```

== Configuring it

What a page may contain is the site's call, in `content { markdown { } }`. Every
key and its accepted values are in the
#link("../configure/reference.typ")[reference]; these are the ones worth knowing
about.

```kdl
content {
  markdown #false          // .md files go back to being files
}

content {
  markdown {
    enabled #true          // what the shorthand above sets
    extensions "smart" "-tables"
    html "drop"
    eval #false
  }
}
```

#table(
  columns: 2,
  align: (left, left),
  table.header([Key], [Does]),
  [`enabled`], [Whether a `.md` file is a page at all. On when the binary has
  the feature; turn it off for a site with `.md` files it does not publish, like
  a README beside its pages.],
  [`extensions`], [Enables one, or `-name` to drop a default. The list adds to
  the defaults rather than replacing them, so naming one extra keeps the rest.],
  [`html`], [`refuse` (the default) or `drop`. Dropping removes an inline run's
  tags and keeps the prose between them; a block-level run is one chunk of HTML,
  so dropping it drops its contents too.],
  [`eval`], [Whether a fence marked `eval` runs at all.],
)

#callout(kind: "warning")[
  `eval #false` is worth setting deliberately for content you did not write. An
  `eval` fence runs arbitrary Typst at build time, and a site accepting
  contributed or imported markdown should not extend that trust to every author.
]

== Turning it off

Two things decide it, and both have to say yes. `.md` support is the `markdown`
cargo feature, on by default and absent from the `slim` build: that is the
*binary's* capability. `content { markdown #false }` is the *site's*, for a
project whose binary has markdown and which still wants its `.md` files left
alone. Either way the file stays where it lies and nothing is published for it.
