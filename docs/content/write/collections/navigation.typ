#let frontmatter = (
  title: "Prev / next links",
  order: 11,
)
#import "/templates/theme.typ": callout

Every content page knows its neighbors. The data is always there, no
configuration, on `page.nav`:

```typ
#let page(page, body) = {
  body
  let nav = page.nav
  if nav.prev != none { link(nav.prev.url)[#nav.prev.title] }
  if nav.next != none { link(nav.next.url)[#nav.next.title] }
}
```

`nav.prev` and `nav.next` are each either `none` or a dict with the neighbor's
`url` and `title`.

== Which neighbors

The order is the #link("defining.typ")[collection's own] `sort` and
`reverse`, so a `sort "date"` `reverse #true` blog links newest to oldest while
a `sort "order"` guide links chapter to chapter. Three things bound it:

- Navigation never crosses a collection boundary, and never crosses a language
  boundary on a multi-language site.
- Drafts, future-dated and expired pages that were filtered out are skipped, so
  a link only ever points at a page that was actually built.
- The 404 page is neither a neighbor nor given neighbors of its own.

Generated #link("pagination.typ")[index] and
#link("taxonomies.typ")[term] pages have no siblings. Their prev/next is
pagination, and it arrives as plain URLs on `page.frontmatter.nav` instead.

== Labels

`page.strings` carries whatever the current language declared under
`languages { fr { strings } }`, so a pager can label itself in the reader's
language. Nothing is declared by default, so read it with a fallback:

```typ
let back = page.strings.at("previous", default: "Previous")
if nav.prev != none { link(nav.prev.url)[#back] }
```

See #link("../i18n.typ")[multiple languages].

#callout(kind: "tip")[
  Reordering a collection moves the pager, the generated index and the sidebar
  together. They all read the same ordered set.
]

== The whole site, in order

`page.nav` names two neighbors, deliberately. For the full ordered tree of
content directories, a template imports `@baudelaire/sections`, which is what a
sidebar is built from:

```typ
#import "@baudelaire/sections:0.1.0": sections
```

See #link("../../lookup/typst-modules.typ")[Typst modules] for its shape.
