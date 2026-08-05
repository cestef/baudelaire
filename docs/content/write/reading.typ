#let frontmatter = (
  title: "Reading time",
  order: 7,
)
#import "/templates/theme.typ": callout

Every page reaches its template knowing how long it takes to read. Nothing to configure, nothing to declare.

```typ
#let page(page, body) = {
  text(fill: gray)[#page.reading.minutes min read]
  body
}
```

#table(
  columns: 3,
  align: (left, left, left),
  table.header([Field], [Type], [Holds]),
  [`page.reading.words`], [int], [The page's word count.],
  [`page.reading.minutes`], [int], [That count at 200 words per minute, rounded up.],
)

An empty page reads as zero minutes, not one.

== Counted from the source

The count comes from the page's Typst source, not its rendered HTML. The render has not happened yet when a template is handed its page, and a #link("collections/pagination.typ")[listing] entry is built earlier still. That makes it an estimate, which a reading time is anyway.

Two consequences:

- Code lines do not count. A line starting with `#` or `//` is machinery (`#import`, `#let`), and counting it inflates a short page most.
- Inline markup counts as the words it reads as. `#emph[one]` is one word, and a heading contributes its words but not its `=`.

#callout(kind: "note")[
  Text pulled in by `#include` is not counted: the estimate reads the page's own source, and an included file is another page's. Neither is a template's chrome, the same answer #link("../build/generate/search.typ")[search] gives when it indexes only `<main>`.
]

== In a listing

Listing rows carry no reading estimate. If a collection index needs one per row, put it in the page's own frontmatter and read it back off `entry.extra`.
