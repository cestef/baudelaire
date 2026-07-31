// The bundle template: the whole manual as one PDF.
//
// Where `print.typ` sets a single page, this is handed every page at once, so a
// title page, a contents list and continuous numbering are yours to decide.
// This is the "download the manual" artifact, the paged counterpart of the
// single-file HTML export.
//
// Nothing uses this file until you ask for it, in `config.kdl`:
//
//   generate { pdf { bundle { template "book.typ"; site #true } } }
//
// The result is `public/site.pdf`. Naming collections instead binds one
// document per collection, at `public/<collection>.pdf`.

#let book(doc, entries) = {
  // `std.page`, not `page`: a bound entry carries a `page` dictionary of its
  // own, and the name is worth keeping free.
  set std.page(
    paper: "a4",
    margin: (x: 2.4cm, top: 2.4cm, bottom: 2cm),
    numbering: "1",
    header: context [
      #set text(size: 9pt, fill: luma(50%))
      #doc.title
    ],
  )
  set text(size: 10.5pt)
  set par(justify: true, leading: 0.62em)
  set heading(numbering: "1.1")
  show raw: set text(size: 9.5pt)

  std.page(numbering: none, header: none, margin: 3cm, align(center + horizon, {
    text(size: 26pt, weight: "bold", doc.title)
    v(0.6em)
    text(size: 11pt, fill: luma(45%), doc.site)
  }))

  outline(title: [Contents], depth: 3)

  for entry in entries {
    pagebreak(weak: true)
    heading(level: 1, entry.page.frontmatter.title)
    if "summary" in entry.page.frontmatter {
      text(fill: luma(35%), entry.page.frontmatter.summary)
      v(0.6em)
    }
    entry.body
  }
}
