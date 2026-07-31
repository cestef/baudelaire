// The bundle template: every chapter bound into one PDF.
//
// Where `print.typ` sets a single page, this is handed the whole run at once,
// so the things only a book can do are yours here: a title page, a contents
// list, continuous numbering, a chapter opening on a fresh sheet.
//
// Nothing uses this file until you ask for it, in `config.kdl`:
//
//   generate { pdf { bundle { template "book.typ"; collections "chapters" } } }
//
// The result is `public/chapters.pdf`. `site #true` binds the whole site
// instead, as `public/site.pdf`.

#let book(doc, entries) = {
  // `std.page`, not `page`: a bound entry carries a `page` dictionary of its
  // own, and the name is worth keeping free.
  set std.page(
    paper: "a5",
    margin: (x: 1.8cm, top: 2cm, bottom: 1.8cm),
    numbering: "1",
  )
  set text(size: 10.5pt)
  set par(justify: true, leading: 0.65em)
  set heading(numbering: "1.1")

  // Title page: no numbering, no running foot.
  std.page(numbering: none, margin: 3cm, align(center + horizon, {
    text(size: 24pt, weight: "bold", doc.title)
    v(0.6em)
    if doc.author != none { text(size: 12pt, doc.author) }
    v(0.3em)
    text(size: 10pt, fill: luma(45%), doc.site)
  }))

  outline(title: [Contents], depth: 2)

  for entry in entries {
    pagebreak(weak: true)
    heading(level: 1, entry.page.frontmatter.title)
    entry.body
  }
}
