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
    tags index=#true template="list.typ"
  }
}
```

Now any page with `tags: ("rust", "cli")` in its frontmatter is grouped
automatically. With `index=#true` you get:

/ #raw("/tags/"): an index of every term with its page count,
/ #raw("/tags/rust/"): a page listing everything tagged `rust`.

Both are ordinary templated pages. They receive the term and its entries as
structured data and render through the `template` you bound, so they inherit the
site layout. The chips under this article link straight into `/tags/`.

By default a taxonomy reads the frontmatter key that matches its own name (the
`tags` taxonomy reads `tags`). Point it at a different key with `key=` to group
the same content more than one way, or to name the taxonomy independently of the
field:

```kdl
content {
  taxonomies {
    tags index=#true template="list.typ"
    topics key="categories" index=#true template="list.typ"
  }
}
```
