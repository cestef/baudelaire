#let post(page, body) = {
  html.elem("article", attrs: (class: "post"))[
    #html.elem("h1", page.frontmatter.title)
    #body
  ]
}
