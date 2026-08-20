// The page a static host serves for an unmatched URL. Bind it from
// `content/404.typ`, which publishes as a flat `404.html`:
//
//   #let frontmatter = (title: "Page not found", template: "not-found.typ")
//
// Named for its export rather than for the file it publishes as: a Typst
// identifier cannot start with a digit, so `404.typ` could never be imported.
//
// The sidebar stays: a reader who mistyped a URL is still inside the manual,
// and the whole of it is one click away. No on-page contents, for the same
// reason the term pages have none.

#import "@baudelaire/html:0.1.0": h
#import "../parts.typ": search-trigger, shell

#let not-found(page, body) = shell(
  page,
  h("article", class: "doc notfound", {
    h("p", class: "notfound-code", aria-hidden: "true", "404")
    h("h1", page.frontmatter.title)
    body
    search-trigger(page)
  }),
  toc: false,
)
