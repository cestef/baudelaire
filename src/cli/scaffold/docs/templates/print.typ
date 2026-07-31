// The paged template: one PDF per page, written beside its HTML, so a section
// of the manual can be handed over as a document.
//
// This is Typst's other target. `html.elem` draws nothing here, and page layout,
// which does nothing in HTML, is the whole job: write this the way you would
// write a document, not a web page. It gets the same `page` dictionary
// `page.typ` gets.
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
    paper: "a4",
    margin: (x: 2.4cm, top: 2.4cm, bottom: 2cm),
    header: context [
      #set text(size: 9pt, fill: luma(50%))
      #site-title
      #h(1fr)
      #page.frontmatter.title
    ],
    footer: context [
      #set text(size: 9pt, fill: luma(50%))
      #h(1fr)
      #counter(std.page).display()
    ],
  )
  set text(size: 10.5pt)
  set par(justify: true, leading: 0.62em)
  // A manual is read by heading, so number them and give them room.
  set heading(numbering: "1.1")
  show heading: set block(above: 1.4em, below: 0.7em)
  show raw: set text(size: 9.5pt)

  text(size: 20pt, weight: "bold", page.frontmatter.title)
  if "summary" in page.frontmatter {
    v(0.5em)
    text(size: 10.5pt, fill: luma(35%), page.frontmatter.summary)
  }
  v(1.4em)
  body
}
