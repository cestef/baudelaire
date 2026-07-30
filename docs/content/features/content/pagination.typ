#let frontmatter = (
  order: 2,
  title: "Listings & pagination",
  tags: ("feature", "content"),
)

== Defining a collection

A `content { collections }` entry groups pages that share sorting, permalinks,
and a default template. It sits under `content` because a collection is a way of
reading the content directory. The node name is the collection id; a leading
string argument is a glob selecting its members, and the keys tune the rest:

```kdl
content {
  collections {
    posts "posts/**/*.typ" sort="date" reverse=#true \
      permalink="/posts/{slug}/" template="post.typ"
  }
}
```

/ #raw("glob"): which files belong to the collection, written either as the
  leading positional string or as `glob="..."`. Omit it and the collection is
  the top-level directory of the same name under `content/`.
/ #raw("sort"), #raw("reverse"): order members by `order` (the default), `date`,
  or `title`, optionally reversed. This is the order the prev/next pager and
  listings follow.
/ #raw("permalink"): the URL template for each member. Tokens `{path}`,
  `{collection}`, `{slug}`, `{year}`, `{month}`, `{day}`, and `{order}` are
  filled per page; the default is `/{path}/{slug}/`. A typo'd template is a
  spanned config error, not a silent fallback.

  `{path}` is where the page sits under `content/`, however deep, so
  `content/guide/deploy/s3.typ` publishes at `/guide/deploy/s3/` and this page's
  own URL carries `features/content/`. `{collection}` is just the collection's
  name, so `/{collection}/{slug}/` flattens every subdirectory onto it. For the
  ordinary layout the two are identical, since a collection *is* a directory;
  they differ once you nest, or once a collection's glob points somewhere not
  named after it.
/ #raw("template"): the default layout for members that don't set their own
  `template` in frontmatter.

== Listings

Give a collection a `list` template and Baudelaire generates an index page at
`/{collection}/` listing its members:

```kdl
content {
  collections {
    features sort="order" list="list.typ"
  }
}
```

That single page holds every member: no pagination. This site's
#link("/features/")[features index] is exactly this: one page, all features, in
`order`.

Add `paginate = N` when a collection is long enough to split:

```kdl
content {
  collections {
    blog sort="date" reverse=#true paginate=5 list="list.typ"
  }
}
```

Now Baudelaire generates `/blog/`, `/blog/page/2/`, `/blog/page/3/`, and so on,
each listing five entries with previous and next links. Pagination is just the
splitting modifier on top of a listing: the same `list` template renders both.

The `page` segment in the URL is configurable per collection with `prefix`:

```kdl
content {
  collections {
    blog paginate=5 prefix="p"    // → /blog/p/2/
    news paginate=5 prefix=""     // → /news/2/
  }
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

Give a collection a `mount` to serve its first listing page at a custom URL: set
`mount="/"` on a blog collection and page 1 becomes the site home, while
`/blog/page/2/` and on keep the normal layout.
