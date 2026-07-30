// A single page or post. The title is a comment line, the way a file opens.

#import "@baudelaire/html:0.1.0": h
#import "../parts.typ": meta-line, pager, shell

#let page(page, body) = shell(page, h("article", class: "post", {
  h("h1", { h("span", class: "hash", aria-hidden: "true", "# "); page.frontmatter.title })
  meta-line(page)
  body
  pager(page)
}))
