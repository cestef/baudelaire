// `@baudelaire/toc`: a table of contents built from the page's own headings.
//
// Generated and served by baudelaire; nothing for this exists on disk. Import
// it as `#import "@baudelaire/toc:0.1.0": toc`.

#import "@baudelaire/html:0.1.0": h

// A table of contents for the page this sits on:
//
//   #toc()
//   #toc(from: 2, to: 4, ordered: true, class: "toc", aria-label: "Contents")
//
// `from` and `to` are heading levels, inclusive. Every other named argument
// becomes an attribute on the `<nav>`, exactly as in `h`.
//
// A page's heading set only exists once it has compiled, so this emits an empty
// `<nav>` and baudelaire fills it afterwards with a nested list of links.
//
// Only headings inside the region `html { region }` names are listed, so a
// layout's own chrome never appears in one, and only those carrying an `id`,
// which is every heading unless `html { anchors }` was narrowed or turned off.
#let toc(from: 2, to: 3, ordered: false, ..attrs) = {
  let marker = (
    (_toc-marker, str(from) + "-" + str(to)),
    (_toc-list, if ordered { "ol" } else { "ul" }),
  ).to-dict()
  h("nav", ..attrs.named(), ..marker)
}
