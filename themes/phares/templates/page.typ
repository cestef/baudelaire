// One documentation page: title, optional lead, body, prev/next.
//
// A template file exports a function named after the file, so this one is bound
// by `template "page.typ"` in a collection or in a page's frontmatter.

#import "@baudelaire/html:0.1.0": h
#import "../parts.typ": breadcrumbs, chips, pager, shell, updated

#let page(page, body) = shell(page, h("article", class: "doc", {
  // Where the page sits in the manual, for a reader who arrived from a search
  // result rather than from the sidebar.
  breadcrumbs(page)
  h("h1", page.frontmatter.title)
  let lead = page.frontmatter.at("summary", default: page.frontmatter.at("description", default: none))
  if lead != none { h("p", class: "lead", lead) }
  body
  chips(page, page.taxonomies.at("tags", default: ()))
  updated(page)
  pager(page)
}))
