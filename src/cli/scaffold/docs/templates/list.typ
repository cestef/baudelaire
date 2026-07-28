#import "@baudelaire/html:0.1.0": h
#import "/templates/theme.typ": pager, shell

// The generated index for a collection (`list=` in `config.kdl`), and for any
// taxonomy you add later. Every entry carries `url`, `label`, an optional
// `date` and `note`, plus the source page's `extra` frontmatter, so one
// template serves an ordered guide index and a dated one alike.
//
// A listing's own `nav` holds bare URLs (page 2 of a paginated index has no
// title), so it is reshaped into what `pager` takes.
#let step(url, label) = if url == none { none } else { (url: url, title: label) }

#let list(page, body) = shell(
  page.frontmatter.title,
  {
    h("ul", class: "listing", for entry in page.frontmatter.entries {
      h("li", h("a", class: "entry", href: entry.url, {
        h("span", class: "entry-title", entry.label)
        let summary = entry.extra.at("summary", default: none)
        if summary != none { h("span", class: "entry-summary", summary) }
      }))
    })
    let nav = page.frontmatter.nav
    pager(
      (prev: step(nav.prev, "Previous"), next: step(nav.next, "Next")),
      label: "Pagination",
    )
  },
  sections: page.sections,
)
