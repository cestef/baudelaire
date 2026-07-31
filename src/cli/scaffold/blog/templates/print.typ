// The paged template: one PDF per page, written beside its HTML, so a post can
// be read on paper or filed away as a document.
//
// This is Typst's other target. `html.elem` draws nothing here, and page layout,
// which does nothing in HTML, is the whole job: write this the way you would
// write a document, not a web page. It gets the same `page` dictionary
// `layout.typ` gets, so anything a post shows on screen it can show here.
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
    margin: (x: 2.2cm, top: 2.4cm, bottom: 2cm),
    footer: context [
      #set text(size: 9pt, fill: luma(45%))
      #site-title
      #h(1fr)
      #counter(std.page).display()
    ],
  )
  set text(size: 11pt)
  set par(justify: true, leading: 0.62em)
  show heading: set block(above: 1.4em, below: 0.7em)

  text(size: 21pt, weight: "bold", page.frontmatter.title)
  // Every page carries `date` as `none` or as both spellings at once: `iso` for
  // a machine, `display` localized for a reader.
  if page.date != none {
    v(0.5em)
    text(size: 9.5pt, fill: luma(45%), page.date.display)
  }
  v(1.4em)
  body
}
