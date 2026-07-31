// Every generated index renders through here: the paginated `/work/` index, the
// term index at /stack/, and each /stack/<term>/ page.
//
// The rows are the same shape the landing page draws its selection from, so an
// index is the same grid with everything in it. The term index lists terms
// rather than projects, and those rows carry no cover or summary: the grid
// renders them as plain cards, which is what a term is.

#import "@baudelaire/html:0.1.0": h
#import "../parts.typ": label, shell, work-grid

#let list(page, body) = shell(
  page,
  {
    h("h1", class: "list-title", page.frontmatter.title)
    work-grid(page, page.frontmatter.entries)

    // Pagination links, present only once a listing splits. These are plain
    // URLs, unlike `page.nav` on a real page, which links whole pages.
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
  },
  wide: true,
)
