// A single page or post. The title is a comment line, the way a file opens.

#import "/.baudelaire/site/html.typ": h
#import "../parts.typ": cover, meta-line, pager, shell

#let page(page, body) = shell(page, h("article", class: "post", {
  h("h1", { h("span", class: "hash", aria-hidden: "true", "# "); page.frontmatter.title })
  meta-line(page)
  cover(page)
  body
  pager(page)
}))
