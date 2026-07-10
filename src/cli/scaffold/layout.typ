// Shared page layout, bound to pages through `template` in their frontmatter or
// the collection config. It receives the page (with its `.frontmatter`) and the
// rendered `body`, and returns the full markup. typst-html wraps this in
// html/head/body automatically, so `set document(title: …)` sets the title.

#let build = sys.inputs.at("baudelaire", default: (:))
#let site = build.at("site", default: (:))

#let layout(page, body) = {
  set document(title: page.frontmatter.title)
  html.elem("link", attrs: (rel: "stylesheet", href: "/assets/style.css"))
  html.elem("header", attrs: (class: "site-header"))[
    #html.elem("a", attrs: (href: "/", class: "brand"), site.at("title", default: "My Site"))
  ]
  html.elem("main")[
    #html.elem("article")[
      #html.elem("h1", page.frontmatter.title)
      #body
    ]
  ]
  html.elem("footer", attrs: (class: "site-footer"))[
    #site.at("author", default: "")
  ]
}
