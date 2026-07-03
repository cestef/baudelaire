#frontmatter((
  title: "Why I built a Typst site generator",
  date: datetime(year: 2026, month: 6, day: 20),
  tags: ("blog", "typst"),
  summary: "Markdown runs out of road fast. Typst already has functions, imports, math, and data loading, so why not build a site generator on it?",
))

I like writing in Typst. It has functions, imports, math, and data loading, the
things Markdown makes you bolt on with a template engine and a dozen plugins. But
there was no good way to turn a folder of `.typ` files into a website.

So Baudelaire treats Typst as the whole toolchain. A page is a `.typ` file. A
layout is a Typst function that takes content and returns markup. There is no
second templating language, because there doesn't need to be one. The compiler
already does everything a template engine pretends to.

What the generator adds is the part a compiler leaves out: a content model
(collections, taxonomies, permalinks), the files a site needs (feeds, sitemap,
search), and a build cache so rebuilds are fast. Everything else is just Typst.

This site is the proof. Read the #link("../guide/quickstart.typ")[quickstart]
to start your own.
