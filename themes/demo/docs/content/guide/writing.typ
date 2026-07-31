#let frontmatter = (
  title: "Writing",
  order: 2,
  summary: "Files, frontmatter, and what the sidebar makes of them.",
  tags: ("basics",),
)

A page is a file. A directory is a section. That is the whole model, and it is
why this manual needs no menu anywhere in its config.

= Frontmatter

```typ
#let frontmatter = (
  title: "Writing",
  order: 2,
  summary: "Files, frontmatter, and what the sidebar makes of them.",
)
```

`order` decides where a page sits among its siblings. Pages without one tie, and
ties fall back to the source path, which is already the order a directory reads
in.

= Headings

Second-level headings become the contents on the right, third-level ones nest
under them. The entry lights up as you reach its section.

== A third-level heading

Like this one, which exists to prove the indent.

== And another

The list is built from the rendered page rather than from the template, because
the headings live in the body and the layout never sees them.

= Tables

#table(
  columns: 3,
  table.header[Field][Required][What it does],
  [`title`], [yes], [the page title, the sidebar entry, the search result],
  [`order`], [no], [position among siblings],
  [`summary`], [no], [the lead under the title, the search snippet],
)
