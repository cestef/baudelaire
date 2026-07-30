#let frontmatter = (
  order: 6,
  title: "Reading time",
  tags: ("feature",),
)
#import "/templates/theme.typ": callout

Every page arrives at its template knowing how long it takes to read. No
configuration and no frontmatter: the data is always there, on `page.reading`.

```typ
#let post(page, body) = {
  text(fill: gray)[#page.reading.minutes min read]
  body
}
```

`reading.words` is the word count and `reading.minutes` the time that implies,
rounded up, at 200 words per minute. A page with nothing in it reads as zero
minutes rather than one.

== Counted from the source

The count comes from the page's *typst source*, not its rendered HTML, because
the source is the only version in hand when a template is handed its page: the
render has not happened yet, and a listing entry is built earlier still.

That makes it an estimate, which a reading time is anyway. Two consequences
worth knowing:

- Code lines do not count. In typst markup a leading `#` starts code, so an
  `#import` or a `#let` is machinery rather than prose, and counting it would
  inflate a short page most.
- Inline markup counts as the words it reads as. `#emph[one]` is one word, and
  a heading contributes its own words but not its `=`.

#callout(kind: "note")[
  Text pulled in by a `#include` is not counted: the estimate reads the page's
  own source, and an included file is another page's. Nor is a template's
  chrome, which is the same answer #link("../discovery/search.typ")[search]
  gives when it indexes only a page's `<main>`.
]
