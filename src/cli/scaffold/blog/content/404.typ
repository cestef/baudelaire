// The page a host serves for a URL that matches nothing. It publishes as a flat
// `404.html`, never as `/404/`, because that is the file a static host looks
// for; it is left out of listings, feeds and the sitemap for the same reason.
#let frontmatter = (
  title: "Not found",
)

That page isn't here. It may have moved, or the link may have been wrong.

- #link("/")[Home]
- #link("/posts/")[All posts]
