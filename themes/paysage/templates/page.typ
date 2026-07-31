// An ordinary page: about, contact, colophon. Title, body, nothing else.
//
// A template file exports a function named after the file, so this one is bound
// by `template "page.typ"` in a collection or in a page's frontmatter.

#import "@baudelaire/html:0.1.0": h
#import "../parts.typ": shell

#let page(page, body) = shell(page, h("article", class: "prose", {
  h("h1", page.frontmatter.title)
  let tagline = page.frontmatter.at("tagline", default: none)
  if tagline != none { h("p", class: "tagline", tagline) }
  body
}))
