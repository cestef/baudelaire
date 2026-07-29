#let frontmatter = (
  order: 3,
  title: "Prev / next navigation",
  tags: ("feature",),
)

Every content page knows its neighbours. Within a collection, the pages sit in
the collection's sort order (ordered guide chapters, dated blog posts) and
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
that were filtered out are skipped: the sibling links only ever point at pages
that were actually built.

This site uses it: the pager under every guide chapter and blog post is exactly
this `page.nav`.

== The whole site, in order

The same build view is available from the `@baudelaire/sections` module, as
`sections(lang)`: the tree of content directories, nested as they sit under
`content/`. Each node is
`(id, pages, children)` where `id` is the directory name, `pages` is its
directly-filed pages as `(url, title)` in sort order, and `children` is its
subdirectories as more nodes. A page under `content/guide/advanced/` lands in
the `advanced` child of the `guide` section, so recurse `children` to walk the
whole tree:

```typ
#import "@baudelaire/sections:0.1.0": sections

#let render(nodes) = {
  for section in nodes {
    for entry in section.pages {
      link(entry.url)[#entry.title]
    }
    render(section.children)   // descend into subdirectories
  }
}

#let page(page, body) = {
  body
  render(sections(page.lang))
}
```

The tree is a module rather than a field on `page` so that the build cache can
tell which pages actually use it. A template that imports it depends on every
title and URL on the site, and so rebuilds when any of them change, which is
correct: its nav really did change. A template that does not import it is
unaffected, so publishing a post recompiles that post and its neighbours rather
than the whole archive.

Unlike the other `@baudelaire/*` modules, this one is served from a file the
build writes under `.baudelaire/`, which is what lets Typst record the import as
an ordinary dependency. That file is machine-local build state: gitignored,
wiped by `clean`, and never edited by hand.

Generated listing pages (taxonomy and pagination indexes) are left out: only
authored content appears. This is how the sidebar on the left is built: it is
not a hand-kept list, so it can never drift from the pages or from the prev/next
pager, which both read from the same ordered set. Reorder a collection's `sort`,
or a page's `order`, and the sidebar and the pager move together.
