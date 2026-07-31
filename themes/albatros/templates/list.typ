// Every generated index renders through here: the paginated collection index,
// the term index at /tags/, and each /tags/<term>/ page. They are one shape (a
// titled page of links), so one file covers all three.
//
// The rows arrive as data and are drawn by the same `entry-row` the home page
// uses, so a generated listing and a hand-built list of pages cannot look like
// two different sites.

#import "@baudelaire/html:0.1.0": h
#import "../parts.typ": entry-list, label, shell

#let list(page, body) = shell(page, {
  h("h1", page.frontmatter.title)
  entry-list(page, page.frontmatter.entries)

  // Pagination links, present only once a listing splits. These are plain URLs,
  // unlike `page.nav` on a real page, which links whole pages.
  let nav = page.frontmatter.nav
  if nav.prev != none or nav.next != none {
    h("nav", class: "pager", aria-label: label(page, "pagination", "Pagination"), {
      if nav.prev != none {
        h("a", class: "prev", rel: "prev", href: nav.prev, {
          h("span", class: "pager-label", label(page, "newer", "Newer"))
        })
      } else {
        h("span")
      }
      if nav.next != none {
        h("a", class: "next", rel: "next", href: nav.next, {
          h("span", class: "pager-label", label(page, "older", "Older"))
        })
      }
    })
  }
})
