#import "/templates/theme.typ": shell, link-to

// Standard content page: title + compiled body inside the site shell, with a
// tag row when the frontmatter declares tags and a prev/next pager linking the
// sibling pages of the collection (ordered guide chapters, dated blog posts).
#let page(page, body) = shell(
  page.frontmatter.title,
  {
    body
    let nav = page.nav
    if nav.prev != none or nav.next != none {
      html.elem("nav", attrs: (class: "pager", "aria-label": "Page navigation"), {
        if nav.prev != none {
          link-to(nav.prev.url, "← " + nav.prev.title)
        } else {
          html.elem("span")
        }
        if nav.next != none {
          link-to(nav.next.url, nav.next.title + " →")
        }
      })
    }
  },
  tags: page.frontmatter.at("tags", default: ()),
  sections: page.sections,
)
