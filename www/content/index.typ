#frontmatter((
  title: "Baudelaire",
  template: "page.typ",
))
#import "/templates/theme.typ": cards, lucide

#html.elem("p", attrs: (class: "lead"))[
  A static site generator that speaks #link("https://typst.app")[Typst]. Write
  your pages as `.typ` files and get incremental builds, clean URLs, taxonomies,
  pagination, feeds, and search. There is no template language to learn, because
  the template language is Typst.
]

#html.elem("p")[
  #html.elem("a", attrs: (class: "cta", href: "guide/quickstart.typ"))[Get started #lucide("arrow-right", size: 16)]
]

== Why Typst instead of Markdown?

Markdown runs out of road quickly. It has no variables, no functions, no math,
and no way to load a CSV and build a table from it. The moment you want any of
that, you reach for a template engine and a pile of plugins.

Typst already has all of it: functions, imports, data loading, math, code
highlighting. So a page is a small program, and a layout is just a function that
takes content and returns markup. Baudelaire adds what a compiler leaves out: a
content model, clean URLs, feeds, and a build cache.

== Explore

#cards((
  ("guide/quickstart.typ", "zap", "Quickstart", "Scaffold, build, and serve in three commands."),
  ("features/search.typ", "search", "Search", "A flat index and a command palette, from one config block."),
  ("features/assets.typ", "package", "Asset pipeline", "Minify CSS, bundle JS, and fingerprint filenames."),
  ("features/taxonomies.typ", "tag", "Taxonomies", "Group pages by tag with generated index pages."),
  ("features/feeds.typ", "rss", "Feeds & sitemap", "RSS, Atom, and sitemap.xml from your dated pages."),
  ("guide/config.typ", "sliders", "Configuration", "Everything the build reads, in one KDL file."),
))

This entire site is built with Baudelaire. The source lives in
#link("https://codeberg.org/cstef/baudelaire")[the repository] under `www/`.
