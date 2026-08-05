#let frontmatter = (
  title: "Baudelaire",
  template: "home.typ",
)
#import "@baudelaire/html:0.1.0": h
#import "@baudelaire/site:0.1.0": lang
#import "@baudelaire/pages:0.1.0": pages
#import "/templates/theme.typ": cards, demo, emit-explorer, lucide

// This site's own page catalogue. Reading it here is the point: the count below
// is the build's, not a number typed into prose.
#let catalogue = pages(lang)

#h("section", class: "hero")[
  #h("p", class: "eyebrow", "Static site generator")
  #h("h1", class: "hero-title")[Write Typst. #h("span", class: "hero-em", "Ship a website.")]
  #h("p", class: "hero-lead")[
    Pages are `.typ` files and layouts are Typst functions. One binary does the
    rest: clean URLs, feeds, taxonomies, search, an asset pipeline, and a build
    cache.
  ]
  #h("a", class: "cta", href: "start/quickstart.typ")[
    Get started #lucide("arrow-right", size: 16)
  ]

  ```sh
  curl -fsSL https://baudelaire.cstef.dev/install.sh -o install.sh
  less install.sh # read it
  sh install.sh
  ```
]

== A page is a program

#demo((
  (
    "Computed",
    "content/sourdough.typ",
    "== Sourdough

#let recipe = (Flour: 300, Water: 210, Salt: 6)
#let total = recipe.values().sum()

#total g of dough, #recipe.len() ingredients.

#table(
  columns: 3,
  table.header([Ingredient], [Grams], [Share]),
  ..recipe.pairs().map(((name, g)) => (
    [#name], [#g], [#calc.round(100 * g / total)%],
  )).flatten(),
)",
  ),
  (
    "Loaded",
    "content/releases.typ",
    "== Shipped

#let rows = csv(\"/data/releases.csv\")

#for (tag, what) in rows [
  / #raw(tag): #what
]",
  ),
  (
    "Written",
    "content/hello.typ",
    "== Hello

Ordinary prose, a #link(\"https://typst.app\")[link],
*bold*, and `code`.

+ Lists, numbered or not
+ Math, set as Typst sets it: $a^2 + b^2 = c^2$
+ Terms, tables, figures, footnotes",
  ),
))

The right pane is not a screenshot. It is the left pane, evaluated while this
page compiled, by the same Typst that built every other page here. That is the
whole argument: Markdown has no variables, no functions, no math, and no way to
load a CSV and build a table from it, so the moment you want one you start
stacking a template engine and plugins on top. What that trade costs, next to
Hugo, Zola, Eleventy and Astro, is on #link("start/compare.typ")[compared].

== One block, every artifact

#emit-explorer((
  (
    id: "sitemap",
    label: "Sitemap",
    note: "Every page, with its last-modified date.",
    kdl: "sitemap #true",
    files: ("sitemap.xml",),
    on: true,
  ),
  (
    id: "feed",
    label: "RSS and Atom",
    note: "Your dated pages, newest first.",
    kdl: "feed { formats \"rss\" \"atom\" }",
    files: ("rss.xml", "atom.xml"),
    on: true,
  ),
  (
    id: "search",
    label: "Search index",
    note: "A flat index, and a command palette to read it.",
    kdl: "search { formats \"json\" }",
    files: ("search.json",),
  ),
  (
    id: "cards",
    label: "Social cards",
    note: "A second, paged compile of each page, drawn by your template.",
    kdl: "cards { template \"card.typ\" }",
    page-files: ("card.png",),
  ),
  (
    id: "pdf",
    label: "A PDF per page",
    note: "Same source, same compiler, different target.",
    kdl: "pdf #true",
    page-files: ("index.pdf",),
  ),
))

Turn one on and the config writes itself, along with what it puts in `dist/`.
Each artifact is built from the pages you already wrote, none of it is on until
the config says so, and each has its own page under
#link("/build/")[Build].

== Start here

#cards((
  ("start/quickstart.typ", "zap", "Quickstart", "Scaffold, build, and serve in three commands."),
  ("write/templates.typ", "package", "Templates", "A layout is a function taking a page and its body."),
  ("configure/reference.typ", "sliders", "Config reference", "Every key the build reads, in one table."),
  ("build/generate/search.typ", "search", "Search", "A flat index, and a palette to read it."),
  ("write/collections/taxonomies.typ", "tag", "Taxonomies", "Group pages by tag, with generated index pages."),
  ("build/generate/feeds.typ", "rss", "Feeds & sitemap", "RSS, Atom, and sitemap.xml from your dated pages."),
))

== Latest writing

// Populated client-side from the build's own `baudelaire:feed` module (no
// fetch, no second source of truth). The link is the no-JS fallback.
#h("div", class: "recent", data-recent: true)[
  #h("a", href: "/blog/", "Read the blog")
]

These docs are a baudelaire site: #catalogue.len() pages, one config file, no
plugins, and the sidebar
on every other page built from the same content tree the build uses for
prev/next links. Its source is in
#link("https://github.com/cestef/baudelaire")[the repository], under `docs/`.
