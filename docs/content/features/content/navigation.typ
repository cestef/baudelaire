#let frontmatter = (
  order: 3,
  title: "Prev / next navigation",
  tags: ("feature",),
)

Every content page knows its neighbours. Within a collection, the pages sit in
the collection's sort order — ordered guide chapters, dated blog posts — and
each one is handed links to the page before and after it. No configuration: the
data is always there for a layout template to use.

The links arrive alongside `frontmatter` and `taxonomies`, on `page.nav`:

```typ
#let page(page, body) = {
  body
  let nav = page.nav
  if nav.prev != none {
    link(nav.prev.url)[← #nav.prev.title]
  }
  if nav.next != none {
    link(nav.next.url)[#nav.next.title →]
  }
}
```

Each of `nav.prev` and `nav.next` is either `none` (at the ends of a collection)
or a dictionary with the neighbour's `url` and `title`. The order follows the
collection's own #link("../guide/config/")[`sort`] and `reverse` settings, so a
`sort="date" reverse=#true` blog links newest-to-oldest, while a
`sort="order"` guide links chapter to chapter.

Navigation never crosses a collection boundary, and drafts or future-dated pages
that were filtered out are skipped — the sibling links only ever point at pages
that were actually built.

This site uses it: the pager under every guide chapter and blog post is exactly
this `page.nav`.

== The whole site, in order

The same build view is available as `page.sections` — every content
collection, in its sort order, as `(id, pages)` where each page is
`(url, title)`:

```typ
#let page(page, body) = {
  body
  for section in page.sections {
    if section.id == "guide" {
      for entry in section.pages {
        link(entry.url)[#entry.title]
      }
    }
  }
}
```

Generated listing pages (taxonomy and pagination indexes) are left out — only
authored content appears. This is how the sidebar on the left is built: it is
not a hand-kept list, so it can never drift from the pages or from the prev/next
pager, which both read from the same ordered set. Reorder a collection's `sort`,
or a page's `order`, and the sidebar and the pager move together.
