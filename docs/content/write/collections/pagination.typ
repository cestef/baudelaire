#let frontmatter = (
  title: "Listings & pagination",
  order: 9,
)
#import "/templates/theme.typ": callout

A `paginate` block generates an index page over a
#link("defining.typ")[collection]. Its presence is what turns the index on.

```kdl
content {
  collections {
    notes {
      sort "order"
      paginate { template "list.typ" }
    }
  }
}
```

That's one page at `/notes/` holding every member, in the collection's sort
order. A collection's own `template` wraps each *member*; the index over them is
a different page and needs its own.

Add a `size` when the collection is long enough to split:

```kdl
content {
  collections {
    blog {
      sort "date"
      reverse #true
      paginate { size 5; template "list.typ" }
    }
  }
}
```

Now the build writes `/blog/`, `/blog/page/2/`, `/blog/page/3/` and so on, five
entries each, with previous and next links. Splitting is a modifier on a
listing, not a separate feature: the same template renders both.

== Keys

#table(
  columns: 4,
  align: (left, left, left, left),
  table.header([Key], [Type], [Default], [Does]),
  [`size`], [int], [--], [Members per index page. Without one, the index is a single page.],
  [`template`], [str], [--], [The layout the index renders through.],
  [`mount`], [str], [`/{collection}/`], [Where page 1 is served.],
  [`prefix`], [str], [`page`], [The path segment before a page number.],
)

`mount "/"` puts a blog on the site root, while `/blog/page/2/` and on keep the
normal layout. An empty `prefix` numbers pages directly under the collection:

```kdl
content {
  collections {
    blog { paginate { size 5; prefix "p" } }
    news { paginate { size 5; prefix "" } }
  }
}
```

#table(
  columns: 3,
  align: (left, left, left),
  table.header([Config], [Page 1], [Page 2]),
  [default], [`/blog/`], [`/blog/page/2/`],
  [`prefix "p"`], [`/blog/`], [`/blog/p/2/`],
  [`prefix ""`], [`/blog/`], [`/blog/2/`],
  [`mount "/"`], [`/`], [`/blog/page/2/`],
)

An index page is titled after its collection, capitalized. Page 2 onwards
appends the localized word for page and the number: `Blog - page 2`.

== What the template receives

The index is an ordinary templated page, so it looks like the rest of your site.
Its data arrives as `page.frontmatter`, structured, never as HTML:

```typ
#let list(page, body) = {
  page.frontmatter.title
  for entry in page.frontmatter.entries {
    link(entry.url)[#entry.label]
  }
  let nav = page.frontmatter.nav
  if nav.next != none { link(nav.next)[Older] }
}
```

Each entry is the same shape everywhere a page appears as data, so one card
component renders a collection index, a
#link("taxonomies.typ")[term page] and a home-page grid:

#table(
  columns: 3,
  align: (left, left, left),
  table.header([Field], [Type], [Holds]),
  [`url`], [str], [The page's permalink.],
  [`label`], [str], [Its title.],
  [`collection`], [str], [The collection it belongs to. Empty for a row with no page behind it.],
  [`lang`], [str], [Its language code.],
  [`date`], [str or none], [Its date as ISO-8601, for a `<time datetime>`.],
  [`display`], [str or none], [The same date written the way the page's language writes one.],
  [`note`], [str or none], [A trailing annotation, such as a term's member count.],
  [`taxonomies`], [dict], [Its terms, keyed by taxonomy.],
  [`extra`], [dict], [Its whole extra #link("../frontmatter.typ")[frontmatter], for summaries and cover images.],
)

#callout(kind: "warn")[
  `page.frontmatter.nav` on a listing carries plain URL strings for the previous
  and next *index* page. `page.nav`, on every page, carries `(url, title)` dicts
  for the neighboring #link("navigation.typ")[content pages]. Two different
  things with similar names.
]

An index generated for an empty collection still gets its page 1, since nav
links point at it and an empty listing beats a 404.
