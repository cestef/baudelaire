#let frontmatter = (
  title: "Baudelaire",
  template: "page.typ",
)
#import "/templates/theme.typ": cards, lucide, link-to

#html.elem("p", attrs: (class: "lead"))[
  A static site generator that speaks #link("https://typst.app")[Typst]. Your
  pages are `.typ` files, your layouts are Typst functions, and one binary does
  the rest.
]

```sh
baudelaire init my-site
cd my-site
baudelaire serve
```

#html.elem("p")[
  #html.elem("a", attrs: (class: "cta", href: "start/quickstart.typ"))[Get started #lucide("arrow-right", size: 16)]
]

== A page

```typ
#let frontmatter = (
  title: "Hello",
  date: datetime(year: 2026, month: 7, day: 30),
  tags: ("typst",),
)

Real Typst: functions, math $x^2$, tables, loaded data.
```

Drop that in `content/blog/hello.typ` and it publishes at `/blog/hello/`, joins
the feeds and the sitemap, shows up under `/tags/typst/`, and gets prev/next
links from its neighbors. None of it is configuration.

== Why not Markdown?

Markdown has no variables, no functions, no math, and no way to load a CSV and
build a table from it. The moment you want one, you're stacking a template
engine and plugins on top.

Typst has all of it already. A page is a small program, a layout is a function
that takes content and returns markup, and there's no second language to learn.
Baudelaire adds what a compiler leaves out: a content model, clean URLs, feeds,
an asset pipeline, and a build cache.

== Start here

#cards((
  ("start/quickstart.typ", "zap", "Quickstart", "Scaffold, build, and serve in three commands."),
  ("write/templates.typ", "package", "Templates", "A layout is a function taking a page and its body."),
  ("configure/overview.typ", "sliders", "Configuration", "Everything the build reads, in one KDL file."),
  ("build/generate/search.typ", "search", "Search", "A flat index and a command palette, from one block."),
  ("write/collections/taxonomies.typ", "tag", "Taxonomies", "Group pages by tag, with generated index pages."),
  ("build/generate/feeds.typ", "rss", "Feeds & sitemap", "RSS, Atom, and sitemap.xml from your dated pages."),
))

== Latest writing

// Populated client-side from the build's own `baudelaire:feed` module (no
// fetch, no second source of truth). The link is the no-JS fallback.
#html.elem("div", attrs: (class: "recent", "data-recent": ""))[
  #link-to("/blog/", "Read the blog")
]

This site is built with Baudelaire. Its source is in
#link("https://github.com/cestef/baudelaire")[the repository] under `docs/`.
