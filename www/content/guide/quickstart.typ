#let frontmatter = (
  title: "Quickstart",
  order: 2,
  tags: ("guide",),
)
#import "/templates/theme.typ": callout

Scaffold a project, then build and preview it:

```sh
baudelaire init my-site
cd my-site
baudelaire serve
```

`init` writes a `config.kdl`, a `content/` tree, a template, and a stylesheet: a
site that already builds. `serve` compiles it, opens a browser, and watches for
changes, live-reloading over Server-Sent Events on every save.

== Write a page

A page is a `.typ` file under `content/`. Frontmatter is a module export — a
top-level `#let frontmatter = (…)` binding — and the rest of the file is the
body. Because it is evaluated by Typst itself, it can be computed, not just a
literal dict:

```typ
#let frontmatter = (
  title: "Hello",
  date: datetime(year: 2026, month: 7, day: 9),
  tags: ("intro",),
)

This is *real Typst*. You have functions, math $x^2$, and
data loading, not just prose.
```

The file's location decides its collection and URL. `content/posts/hello.typ`
joins the `posts` collection and, with clean URLs on, is served at
`/posts/hello/`.

== Build for production

```sh
baudelaire build
```

The finished site lands in `public/`, ready to upload anywhere. Builds are
incremental: unchanged pages come from cache, so a rebuild only touches what you
edited.

#callout(kind: "tip")[
  `baudelaire check` compiles every page and reports broken internal links
  without writing any output. Handy in CI.
]

Next: shape the site in #link("config.typ")[configuration], or jump to
#link("../features/search.typ")[search].
