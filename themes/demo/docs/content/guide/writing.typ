#let frontmatter = (
  title: "Writing",
  order: 2,
  summary: "Files, frontmatter, and what the sidebar makes of them.",
  tags: ("basics",),
)

A page is a file. A directory is a section. This manual needs no menu in its
config.

= Frontmatter

```typ
#let frontmatter = (
  title: "Writing",
  order: 2,
  summary: "Files, frontmatter, and what the sidebar makes of them.",
)
```

`order` sets where a page sits among its siblings. Pages without one tie, and
ties fall back to the source path, which is the order a directory reads in.

= Headings

Second-level headings become the contents on the right, third-level ones nest
under them. The entry lights up as you reach its section.

== A third-level heading

Like this one, which shows the indent.

== And another

The list is built from the rendered page, not the template, because the
headings are in the body and the layout never sees them.

= Tables

#table(
  columns: 3,
  table.header[Field][Required][What it does],
  [`title`], [yes], [the page title, the sidebar entry, the search result],
  [`order`], [no], [position among siblings],
  [`summary`], [no], [the lead under the title, the search snippet],
)
