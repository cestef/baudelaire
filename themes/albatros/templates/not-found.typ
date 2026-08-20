// The page a static host serves for an unmatched URL. Bind it from
// `content/404.typ`, which publishes as a flat `404.html`:
//
//   #let frontmatter = (title: "Not found", template: "not-found.typ")
//
// Named for its export rather than for the file it publishes as: a Typst
// identifier cannot start with a digit, so `404.typ` could never be imported.

#import "@baudelaire/html:0.1.0": h
#import "@baudelaire/sections:0.1.0": sections
#import "../parts.typ": label, shell, titlecase

#let not-found(page, body) = shell(page, h("article", class: "post notfound", {
  h("p", class: "notfound-code", aria-hidden: "true", "404")
  h("h1", page.frontmatter.title)
  body

  // Where to go instead, from the same source as the nav: a reader who lands
  // here wanted a section, and the site's own tree is the list of them.
  let entries = sections(page.lang).filter(s => s.pages.len() > 0 or s.children.len() > 0)
  h("nav", class: "notfound-links", aria-label: label(page, "primary", "Primary"), {
    h("a", class: "chip", href: "/", label(page, "home", "Home"))
    for s in entries { h("a", class: "chip", href: "/" + s.id + "/", titlecase(s.id)) }
  })
}))
