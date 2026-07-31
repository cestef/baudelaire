// The home page: the page's own words, then the newest posts under them.
//
// Bind it from a page's frontmatter (`template: "home.typ"`). The post list is
// read from `@baudelaire/pages`, the build's own catalogue, so it needs no
// second list in config and cannot fall behind what the site publishes.

#import "@baudelaire/html:0.1.0": h
#import "@baudelaire/pages:0.1.0": pages
#import "../parts.typ": entry-list, label, shell

// How many posts the home page shows; `recent` in a page's frontmatter
// overrides it, and the collection is `collection` (default `posts`).
#let _limit = 5

#let home(page, body) = shell(page, {
  h("article", class: "post", {
    h("h1", page.frontmatter.title)
    body
  })

  let collection = page.frontmatter.at("collection", default: "posts")
  let limit = page.frontmatter.at("recent", default: _limit)
  // The catalogue is already in each collection's own sort order, so a
  // reverse-dated blog needs no sorting here: the newest posts are the first.
  let posts = pages(page.lang).filter(entry => entry.collection == collection)
  if posts.len() > 0 {
    h("section", class: "recent", {
      h("h2", label(page, "recent", "Recent posts"))
      entry-list(page, posts.slice(0, calc.min(limit, posts.len())))
      h("p", class: "more", h("a", href: "/" + collection + "/", {
        label(page, "archive", "All posts") + " →"
      }))
    })
  }
})
