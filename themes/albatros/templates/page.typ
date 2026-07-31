// A single page or post: title, byline, body, tags, prev/next.
//
// A template file exports a function named after the file, so this one is bound
// by `template "page.typ"` in a collection or in a page's frontmatter.

#import "@baudelaire/html:0.1.0": h
#import "../parts.typ": byline, chips, pager, shell

#let page(page, body) = shell(page, h("article", class: "post", {
  h("h1", page.frontmatter.title)
  // A dateless page (an about page) gets no byline rather than an empty one.
  byline(page)
  body
  chips(page, page.taxonomies.at("tags", default: ()))
  pager(page)
}))
