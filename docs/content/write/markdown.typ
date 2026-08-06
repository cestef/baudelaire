#let frontmatter = (
  title: "Markdown pages",
  order: 3,
)
#import "/templates/theme.typ": callout

A `.md` file under `content/` is a page, exactly like a `.typ` one. Its
frontmatter is a fenced block at the top of the file, in YAML, TOML or KDL.

```md
---
title: Install
order: 1
tags: [cli, setup]
---

# Install

One binary, **no runtime**.
```

A post copied out of another generator therefore needs no rewriting: `---` is
YAML and `+++` is TOML, exactly as they are everywhere else.

Markdown is a source dialect, not a second pipeline: a page lowers to Typst
before it compiles. Permalinks, collections, taxonomies, link checking,
highlighting, feeds, search and the incremental cache are the ones that were
already there, because by the time the build looks at the page there is nothing
markdown-shaped left.

== Frontmatter

The fence says which language the block is in. There is nothing to configure and
nothing to name after it: a block is never read as a language it is not.

#table(
  columns: 3,
  align: (left, left, left),
  table.header([Fence], [Dialect], [Why that one]),
  [`---`], [YAML], [What every other generator puts between `---`.],
  [`+++`], [TOML], [Zola's and Hugo's.],
  [`;;;`], [KDL], [The language `config.kdl` uses. `;` is KDL's own terminator.],
)

The same page, three ways:

```md
---
title: The night train
date: 2026-08-05
tags: [rail]
---
```

```md
+++
title = "The night train"
date = 2026-08-05
tags = ["rail"]
+++
```

```md
;;;
title "The night train"
date "2026-08-05"
tags "rail" "night"
;;;
```

Every #link("frontmatter.typ")[frontmatter key] behaves as it does on a Typst
page whichever fence you used: the same built-ins, the same collection schema,
the same typo suggester. The block is read into the same dict a `.typ` page
exports, and nothing downstream knows which language it came from.

#callout(kind: "note")[
  *KDL cannot spell a one-element list.* One argument is always the scalar, so
  `tags "rail"` is the string, not a list of one. That is the counterpart of
  Typst's trailing comma in `("rail",)`, and it is why the KDL example above
  carries two tags. YAML (`tags: [rail]`) and TOML (`tags = ["rail"]`) both
  write one without trouble.
]

Dates are the one place the dialects differ in what they can hold. TOML has a
real date literal; YAML and KDL do not, so write the ISO day as a string
(`date: "2026-08-05"`). Either reaches the same reader.

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

An error inside a fence is reported against the line you wrote, not against the
Typst the page lowered to:

`````
x unclosed delimiter
  ,-[content/broken.md:9:10]
8 | ```typ eval
9 | #let x = [
  :          -
`````

== A body from elsewhere

A page's body can be a file the site does not otherwise publish: a `CHANGELOG.md`
at the top of the repository, a `README.md` shared with another project. The
config declares the file under a name, and the page names the name:

```kdl
paths {
  sources {
    changelog "../CHANGELOG.md"
  }
}
```

```md
;;;
title "Changelog"
source "changelog"
;;;
```

That is the whole page: a `source` and a body of its own is an error, since one
of the two would have to be dropped. The file is read as markdown under its own
name, so a fault in it is reported where the prose is.

#callout(kind: "note")[
  A page names a *name*, never a path, and `paths { sources }` is the only place
  a path is written. A page cannot reach a file the config did not offer it, and
  a #link("../start/themes.typ")[theme] cannot add one: `paths` is among the
  sections a theme is refused. The declared path may sit outside the project,
  which is the case this exists for, and that is the config's call to make.
]

Typst pages have no need of it: `#include` reads a file already, and typst tracks
it as a dependency of the page. A `source` on a `.typ` page is refused rather
than ignored.

== A worked example

This site's own #link("../lookup/changelog.md")[changelog] is exactly that: a
five-line frontmatter block whose body is the repository's `CHANGELOG.md`. It
exercises tables, reference links, nested lists and sixty-odd fenced samples, and
no copy of it is kept anywhere.

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

#callout(kind: "warn")[
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
