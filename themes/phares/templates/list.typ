// Every generated index renders through here: the term index at /tags/ and each
// /tags/<term>/ page. A manual's own sections are the sidebar, so the only
// listings a docs site generates are the taxonomy ones.
//
// No on-page contents: a list of links has no headings to collect.

#import "@baudelaire/html:0.1.0": classes, h
#import "../parts.typ": label, shell

#let list(page, body) = shell(
  page,
  h("article", class: "doc", {
    h("h1", page.frontmatter.title)

    h("ul", class: "listing", for entry in page.frontmatter.entries {
      h("li", class: classes("entry", ("dated", entry.date != none)), {
        h("a", class: "entry-title", href: entry.url, entry.label)
        if entry.note != none { h("span", class: "count", entry.note) }
        if entry.description != none {
          h("p", class: "entry-summary", entry.description)
        }
      })
    })

    // Pagination links, present only once a listing splits. These are plain
    // URLs, unlike `page.nav` on a real page, which links whole pages.
    let nav = page.frontmatter.nav
    if nav.prev != none or nav.next != none {
      h("nav", class: "pager", aria-label: label(page, "pagination", "Pagination"), {
        if nav.prev != none {
          h("a", class: "prev", rel: "prev", href: nav.prev, {
            h("span", class: "pager-label", label(page, "newer", "Previous"))
          })
        } else {
          h("span")
        }
        if nav.next != none {
          h("a", class: "next", rel: "next", href: nav.next, {
            h("span", class: "pager-label", label(page, "older", "Next"))
          })
        }
      })
    }
  }),
  toc: false,
)
