// A single page or post: title, byline, body, tags, prev/next.
//
// A template file exports a function named after the file, so this one is bound
// by `template "page.typ"` in a collection or in a page's frontmatter.

#import "@baudelaire/html:0.1.0": h
#import "../parts.typ": chips, pager, posted, reading-badge, shell

#let page(page, body) = shell(page, h("article", class: "post", {
  h("h1", page.frontmatter.title)

  // Dateless pages (an about page) get no byline rather than an empty one.
  let date = posted(page.date)
  let reading = reading-badge(page)
  if date != none or reading != none {
    h("p", class: "byline", {
      date
      if date != none and reading != none { h("span", class: "sep", "·") }
      reading
    })
  }

  body

  chips(page, page.taxonomies.at("tags", default: ()))
  pager(page)
}))
