#let frontmatter = (
  title: "Quickstart",
  order: 2,
)
#import "/templates/theme.typ": callout

Scaffold a project, then build and preview it:

```sh
baudelaire init my-site
cd my-site
baudelaire serve
```

`init` writes a `config.kdl`, a starter page post under `content/`, a
`templates/layout.typ`, and an `assets/style.css`. `baudelaire serve` compiles & watches changes. Live-reloading happenns over Server-Sent Events on every save.

== Write a page

A page is a `.typ` file under `content/`. Front matter is a module export (`#let frontmatter = (…)`) and the rest of the file is the body. Because it is evaluated by Typst itself, it can be computed, not just a literal dict:

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
`/posts/hello/`. For every key frontmatter understands, see
#link("frontmatter.typ")[frontmatter].

== Build for production

```sh
baudelaire build
```

The finished site lands in `public/`, which you can directly hook up to cloudflare, netlify, vercel, github pages you name it. A rebuild only touches what you edited, unchanged pages come from cache.

#callout(kind: "tip")[
  `baudelaire check` compiles every page and reports broken internal links
  without writing any output. Handy in CI.
]
