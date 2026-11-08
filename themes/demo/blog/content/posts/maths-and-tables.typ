#let frontmatter = (
  title: "Maths, tables, and other things prose is bad at",
  date: datetime(year: 2026, month: 7, day: 21),
  tags: ("typst",),
  authors: ("camille",),
  summary: "The parts of a page a Markdown site sends you to a plugin for.",
  image: "/assets/cover-maths.svg",
  alt: "Curves fanning out from a baseline."
)

Typst handles the things Markdown leaves to plugins. None of the following is an
extension.

= An equation

The sum an incremental build computes is how much work it managed not to do:

$ "saved" = sum_(p in "pages") "cost"(p) dot [p "unchanged"] $

Inline maths sits in a sentence: a page whose fingerprint $h(p)$ is unchanged
is served from the cache.

= A table

#table(
  columns: 3,
  table.header[Flavor][Binary][Drops],
  [full], [about 40 MiB], [nothing],
  [slim], [about 11 MiB], [fonts, JS, CSS, images, cards],
)

= A numbered list

+ Parse the content tree and read every frontmatter.
+ Fingerprint each page against the cache.
+ Compile what changed, in parallel.
+ Post-process the DOM: links, anchors, images, meta.
+ Write, then sweep whatever no source maps to.
