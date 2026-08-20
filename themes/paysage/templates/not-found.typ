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

#let not-found(page, body) = shell(page, h("article", class: "prose notfound", {
  h("p", class: "notfound-code", aria-hidden: "true", "404")
  h("h1", page.frontmatter.title)
  body

  // Where to go instead, from the same source as the nav: the site's own
  // directories, so this page cannot fall behind them.
  let entries = sections(page.lang).filter(s => s.pages.len() > 0 or s.children.len() > 0)
  h("p", class: "hero-links", {
    h("a", class: "hero-link", href: "/", label(page, "home", "Home"))
    for s in entries {
      h("a", class: "hero-link", href: "/" + s.id + "/", titlecase(s.id))
    }
  })
}))
