// One project, as a case study: cover, a fact table drawn from the project's
// own frontmatter, the write-up, then the next project.

#import "@baudelaire/html:0.1.0": h
#import "../parts.typ": label, next-project, posted, shell, titlecase

// The facts a project page shows beside the date, in this order, read from the
// page's own frontmatter. A field the page does not set is simply absent, so a
// site adds `client` or drops `role` by writing frontmatter, not by editing
// this file.
#let _facts = ("role", "client", "duration")

#let project(page, body) = shell(page, h("article", class: "project", {
  h("header", class: "project-head", {
    h("h1", page.frontmatter.title)
    let summary = page.frontmatter.at("summary", default: none)
    if summary != none { h("p", class: "tagline", summary) }

    h("dl", class: "facts", {
      let date = posted(page.date)
      if date != none {
        h("div", class: "fact", {
          h("dt", label(page, "date", "Date"))
          h("dd", date)
        })
      }
      for key in _facts {
        let value = page.frontmatter.at(key, default: none)
        if value != none {
          h("div", class: "fact", {
            h("dt", label(page, key, titlecase(key)))
            h("dd", value)
          })
        }
      }
      let terms = page.taxonomies.at("stack", default: ())
      if terms.len() > 0 {
        h("div", class: "fact", {
          h("dt", label(page, "stack", "Stack"))
          h("dd", class: "fact-stack", for term in terms {
            h("a", class: "chip", href: "/stack/" + term + "/", term)
          })
        })
      }
    })
  })

  let cover = page.frontmatter.at("cover", default: none)
  if cover != none {
    h("figure", class: "cover", h("img", src: cover, alt: page.frontmatter.title))
  }

  h("div", class: "prose", body)
  next-project(page)
}))
