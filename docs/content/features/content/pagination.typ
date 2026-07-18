#let frontmatter = (
  order: 2,
  title: "Listings & pagination",
  tags: ("feature", "content"),
)

Give a collection a `list` template and Baudelaire generates an index page at
`/{collection}/` listing its members:

```kdl
collections {
  features sort="order" list="list.typ"
}
```

That single page holds every member: no pagination. This site's
#link("/features/")[features index] is exactly this: one page, all features, in
`order`.

Add `paginate = N` when a collection is long enough to split:

```kdl
collections {
  blog sort="date" reverse=#true paginate=5 list="list.typ"
}
```

Now Baudelaire generates `/blog/`, `/blog/page/2/`, `/blog/page/3/`, and so on,
each listing five entries with previous and next links. Pagination is just the
splitting modifier on top of a listing: the same `list` template renders both.

The `page` segment in the URL is configurable per collection with `prefix`:

```kdl
collections {
  blog paginate=5 prefix="p"    // → /blog/p/2/
  news paginate=5 prefix=""     // → /news/2/
}
```

An empty `prefix` drops the segment entirely, numbering pages directly under the
collection.

The `list` template receives the page's entries and its navigation as structured
data, not HTML:

```typ
#let list(page, body) = {
  for entry in page.frontmatter.entries {
    // entry.url, entry.label, entry.date, entry.note, entry.extra, entry.taxonomies
  }
  let nav = page.frontmatter.nav   // nav.prev, nav.next
}
```

Each entry carries the source page's `date`, its full `extra` frontmatter, and
its `taxonomies` (a dict of taxonomy name to its terms), so a blog index can show
dates and tags while a tag index shows counts, all from one template. Because it
is a template, paginated indexes look like the rest of your site. This site's
#link("/blog/")[blog] is paginated exactly this way.

Give a collection an `index` to mount its first listing page at a custom URL: set
`index="/"` on a blog collection and page 1 becomes the site home, while
`/blog/page/2/` and on keep the normal layout.
