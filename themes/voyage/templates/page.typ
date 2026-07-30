// A single page or post: title, byline, body, tags, prev/next.

#import "@baudelaire/html:0.1.0": h
#import "../parts.typ": byline, chips, pager, shell

#let page(page, body) = shell(page, h("article", class: "post", {
  h("h1", page.frontmatter.title)
  byline(page)
  body
  chips(page, page.taxonomies.at("tags", default: ()))
  pager(page)
}))
