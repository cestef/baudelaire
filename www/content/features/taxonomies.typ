#frontmatter((
  title: "Taxonomies",
  tags: ("feature", "content"),
))

A taxonomy groups pages by a shared frontmatter list: tags, categories, a series.
Declare one and Baudelaire generates the index pages for it.

```kdl
taxonomies {
  tags kind="list" index=#true template="list.typ"
}
```

Now any page with `tags: ("rust", "cli")` in its frontmatter is grouped
automatically. With `index=#true` you get:

/ #raw("/tags/"): an index of every term with its page count,
/ #raw("/tags/rust/"): a page listing everything tagged `rust`.

Both are ordinary templated pages. They receive the term and its entries as
structured data and render through your `list` template, so they inherit the site
layout. The chips under this article link straight into `/tags/`.

`kind` is `list` for flat terms or `tree` for nested ones (a series with parts).
Point several taxonomies at different frontmatter keys to group the same content
more than one way.
