// The paged template: one PDF per chapter, written beside its HTML.
//
// This is Typst's other target, and for a book it is the obvious one: the same
// chapter you read in a browser, set on paper. `html.elem` draws nothing here,
// and page layout, which does nothing in HTML, is the whole job. It gets the
// same `page` dictionary `chapter.typ` gets.
//
// Nothing uses this file until you ask for it, in `config.kdl`:
//
//   generate { pdf { pages { template "print.typ" } } }
//
// `baudelaire init --with pdf` writes that line for you.

#import "@baudelaire/site:0.1.0": title as site-title

#let print(page, body) = {
  // `std.page`, not `page`: the parameter above shadows Typst's own element,
  // and a bare `set page(..)` fails with `expected function, found dictionary`.
  set std.page(
    paper: "a5",
    margin: (x: 1.8cm, top: 2cm, bottom: 1.8cm),
    footer: context [
      #set text(size: 8.5pt, fill: luma(45%))
      #site-title
      #h(1fr)
      #counter(std.page).display()
    ],
  )
  set text(size: 10.5pt)
  set par(justify: true, first-line-indent: 1.2em, leading: 0.65em)
  show heading: set block(above: 1.4em, below: 0.7em)

  // A chapter opens on its own title rather than a running head.
  align(center, {
    text(size: 18pt, weight: "bold", page.frontmatter.title)
    v(0.4em)
    line(length: 30%, stroke: 0.5pt + luma(70%))
  })
  v(1.6em)
  body
}
