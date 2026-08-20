// The page a static host serves for an unmatched URL, written the way a shell
// answers a path that is not there. Bind it from `content/404.typ`, which
// publishes as a flat `404.html`:
//
//   #let frontmatter = (title: "No such file or directory", template: "not-found.typ")
//
// Named for its export rather than for the file it publishes as: a Typst
// identifier cannot start with a digit, so `404.typ` could never be imported.

#import "@baudelaire/html:0.1.0": h
#import "@baudelaire/sections:0.1.0": sections
#import "../parts.typ": shell

#let not-found(page, body) = shell(page, h("article", class: "post notfound", {
  h("h1", class: "error", {
    h("span", class: "code", "404")
    ": " + page.frontmatter.title
  })
  body

  // Where to go instead, as the listing the failed command would have printed.
  h("p", class: "cmd-line", {
    h("span", class: "sigil", aria-hidden: "true", "$ ")
    h("span", "ls /")
  })
  let entries = sections(page.lang).filter(s => s.pages.len() > 0 or s.children.len() > 0)
  h("ul", class: "listing", for s in entries {
    h("li", class: "entry", h("a", class: "entry-title", href: "/" + s.id + "/", s.id + "/"))
  })
  h("p", class: "cmd-line", h("a", class: "cmd", href: "/", "cd /"))
}))
