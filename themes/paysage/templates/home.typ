// The landing page: a hero in the page's own words, then the work.
//
// Bind it from the page's frontmatter (`template: "home.typ"`). The selection
// is read from `@baudelaire/pages`, the build's own catalogue, so adding a
// project publishes it here with no second list to keep in step.

#import "@baudelaire/html:0.1.0": h
#import "@baudelaire/pages:0.1.0": pages
#import "../parts.typ": hero, label, shell, work-grid

// How many projects the landing page shows; `selected` in the page's
// frontmatter overrides it, and the collection is `collection` (default `work`).
#let _selected = 6

#let home(page, body) = shell(
  page,
  {
    hero(page, body)

    let collection = page.frontmatter.at("collection", default: "work")
    let limit = page.frontmatter.at("selected", default: _selected)
    // The catalogue arrives in each collection's own sort order, so a
    // reverse-dated `work` collection needs no sorting here: newest first.
    let projects = pages(page.lang).filter(entry => entry.collection == collection)
    if projects.len() > 0 {
      h("section", class: "work", {
        h("h2", class: "section-title", label(page, "selected", "Selected work"))
        work-grid(page, projects.slice(0, calc.min(limit, projects.len())))
        if projects.len() > limit {
          h("p", class: "more", h("a", href: "/" + collection + "/", {
            label(page, "all-work", "All work") + " →"
          }))
        }
      })
    }
  },
  wide: true,
)
