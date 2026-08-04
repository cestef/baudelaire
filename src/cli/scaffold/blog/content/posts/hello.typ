#let frontmatter = (
  title: "Hello World",
  slug: "hello-world",
  date: datetime(year: 2024, month: 1, day: 1),
  // A single-element Typst array needs the trailing comma. Without it this is
  // the string "intro", not a list of one tag.
  tags: ("intro", "typst"),
  // The one-line summary. It fills the meta description, the feed entry and
  // the search index, and listings read it as `entry.description`.
  description: "What you get when a post is a Typst document rather than markdown.",
)

This is a post. It is also a Typst document, which is a larger claim than it
sounds: the page has functions, imports, variables, and the whole standard
library, so anything you would otherwise hand-write in HTML can be computed
instead.

= It is a real language

Define something once and use it:

#let half-life(name, years) = [#name decays with a half-life of #years years.]

#half-life("Carbon-14", 5730)

Data loading works the same way, so a table of results can come from the CSV you
already have rather than from markup you maintain by hand.

= And it typesets

Math is first class, $e^(i pi) + 1 = 0$, and so are the pieces a document
actually needs: figures with captions, cross references, footnotes. The HTML
export keeps them as semantic elements rather than pictures of themselves.

= Frontmatter drives the build

The `date` above puts this post in the feed and orders it in the archive; the
`tags` put it under #link("/tags/intro/")[intro] and
#link("/tags/typst/")[typst]. Delete this file once you have written something
of your own.
