// The listing template. Every generated index page renders through it: the
// paginated /posts/, the /tags/ term index, and each /tags/<term>/ page. They
// are the same shape (a titled page of links), so one file covers all three.
//
// The entries arrive as data, not as markup: each carries `url`, `label`, an
// optional `date`, an optional `note` (a taxonomy term's member count), the
// source page's `taxonomies`, and its whole `extra` frontmatter. That is why a
// dated post index and an undated tag index can share this template.

#import "@baudelaire/html:0.1.0": h, classes
#import "/templates/layout.typ": layout

// Listings reuse the site shell, so they never drift from a hand-written page.
#let list(page, body) = layout(page, {
  h("ul", class: "listing", for entry in page.frontmatter.entries {
    // `classes` joins names and takes a (name, condition) pair for a
    // conditional one. Writing `"entry" + if dated { " dated" }` looks
    // equivalent but yields none on the else branch and fails the build.
    h("li", class: classes("entry", ("dated", entry.date != none)), {
      h("a", class: "entry-title", href: entry.url, entry.label)
      if entry.date != none { h("time", class: "meta", entry.date) }
      if entry.note != none { h("span", class: "meta", entry.note) }
      // Anything else a post put in its frontmatter is here under `extra`.
      let summary = entry.extra.at("summary", default: none)
      if summary != none { h("p", class: "entry-summary", summary) }
    })
  })

  // Pagination navigation, present only once a listing splits. These are plain
  // URLs, unlike `page.nav` in layout.typ, which links whole pages.
  let nav = page.frontmatter.nav
  if nav.prev != none or nav.next != none {
    h("nav", class: "pager", aria-label: "Pagination", {
      if nav.prev != none {
        h("a", rel: "prev", href: nav.prev)[← Newer]
      } else {
        h("span")
      }
      if nav.next != none { h("a", rel: "next", href: nav.next)[Older →] }
    })
  }
})
