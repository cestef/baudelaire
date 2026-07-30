// Every generated index renders through here: the paginated collection index,
// the term index at /tags/, and each /tags/<term>/ page.
//
// Entry dates arrive in both forms: `date` is the machine one for the
// `datetime` attribute, `display` the one written the way the page's language
// writes it. A listing shows the second.

#import "@baudelaire/html:0.1.0": classes, h
#import "../parts.typ": label, shell

#let list(page, body) = shell(page, {
  h("h1", page.frontmatter.title)

  h("ul", class: "listing", for entry in page.frontmatter.entries {
    h("li", class: classes("entry", ("dated", entry.date != none)), {
      h("a", class: "entry-title", href: entry.url, entry.label)
      if entry.date != none {
        h("time", class: "date", datetime: entry.date, entry.display)
      }
      if entry.note != none { h("span", class: "count", entry.note) }
      let summary = entry.extra.at("summary", default: none)
      if summary != none { h("p", class: "entry-summary", summary) }
      let terms = entry.taxonomies.at("tags", default: ())
      if terms.len() > 0 {
        h("span", class: "entry-tags", terms.join(" · "))
      }
    })
  })

  let nav = page.frontmatter.nav
  if nav.prev != none or nav.next != none {
    h("nav", class: "pager", aria-label: label(page, "pagination", "Pagination"), {
      if nav.prev != none {
        h("a", class: "prev", rel: "prev", href: nav.prev, label(page, "newer", "Newer"))
      } else {
        h("span")
      }
      if nav.next != none {
        h("a", class: "next", rel: "next", href: nav.next, label(page, "older", "Older"))
      }
    })
  }
})
