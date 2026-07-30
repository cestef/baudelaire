#let frontmatter = (
  title: "Typst is the template",
  date: datetime(year: 2026, month: 7, day: 12),
  tags: ("typst", "templates"),
  summary: "Why the content and the layout being one language is the whole point.",
)

Most site generators ask you to hold two languages at once: one for prose, one
for the markup around it. The seam between them is where the awkwardness lives.
A loop belongs to the template language, a footnote to the content language, and
a footnote inside a loop belongs to an argument.

= One language

Here there is no seam. A heading is a heading, and the layout that wraps it is
written in the same language, with the same functions:

```typ
#let page(page, body) = h("article", {
  h("h1", page.frontmatter.title)
  body
})
```

That template is not a string being assembled. It builds real elements, which is
why an attribute with a hyphen, an ARIA role, or an inline `<svg>` needs no
escape hatch.

= What that buys

- Frontmatter is a Typst binding, so the compiler sees it and a typo in a field
  name is an error rather than a silent `none`.
- A helper you write for a page works in a template, and the other way round.
- Anything Typst can set, a page can contain: maths, tables, footnotes#footnote[Like this one.], figures.

#quote(block: true, attribution: [the README])[
  Your pages, your layouts, and your data are one language, so a heading, a
  footnote, and a nav menu are all written the same way.
]
