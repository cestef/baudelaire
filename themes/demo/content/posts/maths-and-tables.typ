#let frontmatter = (
  title: "Maths, tables, and other things prose is bad at",
  date: datetime(year: 2026, month: 7, day: 21),
  tags: ("typst",),
  summary: "The parts of a page a Markdown site sends you to a plugin for.",
)

A typesetting language brings its own answers to the things Markdown farms out
to plugins. None of the following is an extension; it is what Typst does.

= An equation

The sum every incremental build is really computing, which is how much work it
managed not to do:

$ "saved" = sum_(p in "pages") "cost"(p) dot [p "unchanged"] $

Inline maths sits in a sentence without ceremony: a page whose fingerprint
$h(p)$ is unchanged is served from the cache.

= A table

#table(
  columns: 3,
  table.header[Flavor][Binary][Drops],
  [full], [about 40 MiB], [nothing],
  [slim], [about 11 MiB], [fonts, JS, CSS, images, cards],
)

= A list that means something

+ Parse the content tree and read every frontmatter.
+ Fingerprint each page against the cache.
+ Compile what changed, in parallel.
+ Post-process the DOM: links, anchors, images, meta.
+ Write, then sweep whatever no source maps to.
