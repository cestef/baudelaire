#import "@baudelaire/html:0.1.0": h
#import "/templates/theme.typ": link-to, shell

// Generated listing page (taxonomy terms, paginated indexes). Each entry
// carries `url`, `label`, an optional `date` and `note`, and the source page's
// `extra` frontmatter, so a dated blog index can show dates and summaries
// while a tag index shows counts, all from one template.
#let list(page, body) = shell(page.frontmatter.title, {
  h("ul", class: "listing", for entry in page.frontmatter.entries {
    let summary = entry.extra.at("summary", default: none)
    h("li", class: "entry", h("a", href: entry.url, class: "entry-link", {
      h("span", class: "entry-head", {
        h("span", class: "entry-title", entry.label)
        if entry.date != none { h("time", class: "entry-date", entry.date) }
        if entry.note != none { h("span", class: "count", entry.note) }
      })
      if summary != none { h("span", class: "entry-summary", summary) }
    }))
  })
  let nav = page.frontmatter.nav
  if nav.prev != none or nav.next != none {
    h("nav", class: "pager", aria-label: "Pagination", {
      if nav.prev != none { link-to(nav.prev, "Previous") } else { h("span") }
      if nav.next != none { link-to(nav.next, "Next") }
    })
  }
}, sections: page.sections)
