// The home page: the page's own words, then the newest posts listed the way
// this theme lists everything, under the command that would have produced them.
//
// Bind it from a page's frontmatter (`template: "home.typ"`). The posts come
// from `@baudelaire/pages`, the build's own catalogue, so the list cannot fall
// behind what the site publishes.

#import "@baudelaire/html:0.1.0": h
#import "@baudelaire/pages:0.1.0": pages
#import "../parts.typ": label, shell

// How many posts the home page shows; `recent` in a page's frontmatter
// overrides it, and the collection is `collection` (default `posts`).
#let _limit = 8

#let home(page, body) = shell(page, {
  h("article", class: "post", {
    h("h1", { h("span", class: "hash", aria-hidden: "true", "# "); page.frontmatter.title })
    body
  })

  let collection = page.frontmatter.at("collection", default: "posts")
  let limit = page.frontmatter.at("recent", default: _limit)
  // The catalogue arrives in each collection's own sort order, so a
  // reverse-dated blog needs no sorting here: the newest posts are the first.
  let posts = pages(page.lang).filter(entry => entry.collection == collection)
  if posts.len() > 0 {
    h("section", class: "recent", {
      h("p", class: "cmd-line", {
        h("span", class: "sigil", aria-hidden: "true", "$ ")
        h("span", "ls -t " + collection + "/")
      })
      h("ul", class: "listing dated", for entry in posts.slice(0, calc.min(limit, posts.len())) {
        h("li", class: "entry", {
          if entry.date != none {
            h("time", class: "date", datetime: entry.date, entry.date)
          }
          h("a", class: "entry-title", href: entry.url, entry.label)
        })
      })
      h("p", class: "cmd-line", h("a", class: "cmd", href: "/" + collection + "/", {
        "cd " + collection
      }))
    })
  }
})
