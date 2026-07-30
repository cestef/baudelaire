#let frontmatter = (
  order: 1,
  title: "Taxonomies",
  tags: ("feature", "content"),
)

A taxonomy groups pages by a shared frontmatter list: tags, categories, a series.
Declare one and Baudelaire generates the index pages for it.

```kdl
content {
  taxonomies {
    tags listing=#true template="list.typ"
  }
}
```

Now any page with `tags: ("rust", "cli")` in its frontmatter is grouped
automatically. With `listing=#true` you get:

/ #raw("/tags/"): an index of every term with its page count,
/ #raw("/tags/rust/"): a page listing everything tagged `rust`.

Both are ordinary templated pages. They receive the term and its entries as
structured data and render through the `template` you bound, so they inherit the
site layout. The chips under this article link straight into `/tags/`.

== Paginating a term

A term listing holds every page under it, which on a blog with three years of
`#rust` posts is one page listing four hundred. `paginate=` chunks it the same
way a #link("pagination.typ")[collection index] is chunked, and by the same
rule, so page 2 is named the same way in both:

```kdl
content {
  taxonomies {
    tags listing=#true paginate=20
  }
}
```

`/tags/rust/` keeps the first twenty, `/tags/rust/page/2/` takes the next, and
each carries `page.nav.prev`/`page.nav.next` like any other paginated listing.
`prefix=` renames the `page` segment, or empties it for `/tags/rust/2/`.

By default a taxonomy reads the frontmatter key that matches its own name (the
`tags` taxonomy reads `tags`). Point it at a different key with `key=` to group
the same content more than one way, or to name the taxonomy independently of the
field:

```kdl
content {
  taxonomies {
    tags listing=#true template="list.typ"
    topics key="categories" listing=#true template="list.typ"
  }
}
```
