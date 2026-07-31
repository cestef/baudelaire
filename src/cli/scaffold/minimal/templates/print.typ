// The paged template: one PDF per page, written beside its HTML.
//
// This is Typst's other target. `html.elem` draws nothing here, and page layout,
// which does nothing in HTML, is the whole job: write this the way you would
// write a document, not a web page. It gets the same `page` dictionary
// `layout.typ` gets.
//
// Nothing uses this file until you ask for it, in `config.kdl`:
//
//   generate { pdf { pages { template "print.typ" } } }
//
// `baudelaire init --with pdf` writes that line for you.

#let print(page, body) = {
  // `std.page`, not `page`: the parameter above shadows Typst's own element,
  // and a bare `set page(..)` fails with `expected function, found dictionary`.
  set std.page(paper: "a4", margin: 2.2cm, numbering: "1")
  set text(size: 11pt)
  set par(justify: true)

  text(size: 20pt, weight: "bold", page.frontmatter.title)
  v(1.2em)
  body
}
