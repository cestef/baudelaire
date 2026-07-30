// Every generated index renders through here: the paginated collection index,
// the term index at /tags/, and each /tags/<term>/ page. They are one shape (a
// titled page of links), so one file covers all three.
//
// The entries arrive as data: `url`, `label`, an optional `date` (machine) and
// `display` (localized), an optional `note` (a term's member count), the source
// page's `taxonomies`, and its whole `extra` frontmatter.

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
    })
  })

  // Pagination links, present only once a listing splits. These are plain URLs,
  // unlike `page.nav` on a real page, which links whole pages.
  let nav = page.frontmatter.nav
  if nav.prev != none or nav.next != none {
    h("nav", class: "pager", aria-label: label(page, "pagination", "Pagination"), {
      if nav.prev != none {
        h("a", class: "prev", rel: "prev", href: nav.prev, "← " + label(page, "newer", "Newer"))
      } else {
        h("span")
      }
      if nav.next != none {
        h("a", class: "next", rel: "next", href: nav.next, label(page, "older", "Older") + " →")
      }
    })
  }
})
