#let frontmatter = (
  title: "Documentation",
  description: "Everything you need to run and extend this site.",
)
#import "/templates/theme.typ": callout

Welcome. This is a documentation site built with
#link("https://baudelaire.cstef.dev")[Baudelaire]: every page is a Typst file
under `content/`, compiled to HTML and dropped into the layout you can read in
`templates/`.

Start with #link("/content/guide/getting-started.typ")[Getting started], then
#link("/content/guide/configuration.typ")[Configuration] when you want to change
how the site is put together.

#callout(kind: "note")[
  Those two links point at `.typ` source paths rather than URLs. Baudelaire
  resolves them to the real permalink and fails the build if the file is gone,
  so a renamed page can never leave a dead link behind.
]

== How the pieces fit

/ `content/`: the pages. A file's path becomes its URL, and `content/guide/`
  is the `guide` collection declared in `config.kdl`.
/ `templates/`: the layout. `theme.typ` holds the shared shell, `page.typ`
  renders a documentation page, `list.typ` renders the generated `/guide/`
  index.
/ `assets/style.css`: the stylesheet, minified and content-hashed on a
  production build.
/ `config.kdl`: what a page is, how pages are ordered, and what extra files the
  build emits.

Everything in the sidebar comes from `sections(page.lang)`, which the build derives
from `content/` itself. Add a file, and it appears. Nothing to register.

== Search

Press `/` or `Ctrl-K`. The index is `/search.json`, written at
build time from the rendered text of every page, and the palette that searches
it is `/search.js`, also generated. Both are switched on by the
`generate { search { .. } }` block in `config.kdl`.
