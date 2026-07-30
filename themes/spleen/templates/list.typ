// Every generated index renders through here: the paginated collection index,
// the term index at /tags/, and each /tags/<term>/ page. One shape, three uses.
//
// Entries are laid out as a directory listing: the date in a fixed column, then
// the name. An undated listing (the term index) drops the column entirely
// rather than leaving a ragged gap.

#import "@baudelaire/html:0.1.0": classes, h
#import "../parts.typ": label, shell

#let list(page, body) = shell(page, {
  h("h1", { h("span", class: "hash", aria-hidden: "true", "# "); page.frontmatter.title })

  let entries = page.frontmatter.entries
  h("ul", class: classes("listing", ("dated", entries.any(e => e.date != none))), for entry in entries {
    h("li", class: "entry", {
      if entry.date != none {
        h("time", class: "date", datetime: entry.date, entry.date)
      }
      h("a", class: "entry-title", href: entry.url, entry.label)
      if entry.note != none { h("span", class: "count", "(" + entry.note + ")") }
      let summary = entry.extra.at("summary", default: none)
      if summary != none { h("span", class: "entry-summary", summary) }
    })
  })

  // Pagination links, present only once a listing splits. These are plain URLs,
  // unlike `page.nav` on a real page, which links whole pages.
  let nav = page.frontmatter.nav
  if nav.prev != none or nav.next != none {
    h("nav", class: "pager", aria-label: label(page, "pagination", "Pagination"), {
      if nav.prev != none {
        h("a", class: "prev", rel: "prev", href: nav.prev, "<- " + label(page, "newer", "newer"))
      } else {
        h("span")
      }
      if nav.next != none {
        h("a", class: "next", rel: "next", href: nav.next, label(page, "older", "older") + " ->")
      }
    })
  }
})
