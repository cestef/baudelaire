#import "/templates/theme.typ": link-to, shell

// Generated listing page (taxonomy terms, paginated indexes). Each entry
// carries `url`, `label`, an optional `date` and `note`, and the source page's
// `extra` frontmatter — so a dated blog index can show dates and summaries
// while a tag index shows counts, all from one template.
#let list(page, body) = shell(page.frontmatter.title, {
  html.elem(
    "ul",
    attrs: (class: "listing"),
    for entry in page.frontmatter.entries {
      let summary = entry.extra.at("summary", default: none)
      html.elem("li", attrs: (class: "entry"), html.elem("a", attrs: (href: entry.url, class: "entry-link"), {
        html.elem("span", attrs: (class: "entry-head"), {
          html.elem("span", attrs: (class: "entry-title"), entry.label)
          if entry.date != none {
            html.elem("time", attrs: (class: "entry-date"), entry.date)
          }
          if entry.note != none {
            html.elem("span", attrs: (class: "count"), entry.note)
          }
        })
        if summary != none {
          html.elem("span", attrs: (class: "entry-summary"), summary)
        }
      }))
    },
  )
  let nav = page.frontmatter.nav
  if nav.prev != none or nav.next != none {
    html.elem("nav", attrs: (class: "pager", "aria-label": "Pagination"), {
      if nav.prev != none { link-to(nav.prev, "Previous") } else { html.elem("span") }
      if nav.next != none { link-to(nav.next, "Next") }
    })
  }
}, sections: page.sections)
