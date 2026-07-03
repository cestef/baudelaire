#frontmatter((
  title: "Pagination",
  tags: ("feature", "content"),
))

Long collections split into numbered index pages. Set `paginate` on a collection
and give it a `list` template:

```kdl
collections {
  blog sort="date" reverse=#true paginate=5 list="list.typ"
}
```

Baudelaire then generates `/blog/`, `/blog/page/2/`, `/blog/page/3/`, and so on,
each listing five entries with previous and next links.

The `list` template receives the page's entries and its navigation as structured
data, not HTML:

```typ
#let list(page, body) = {
  for entry in page.frontmatter.entries {
    // entry.url, entry.label, entry.date, entry.note, entry.extra
  }
  let nav = page.frontmatter.nav   // nav.prev, nav.next
}
```

Each entry carries the source page's `date` and its full `extra` frontmatter, so
a blog index can show dates and summaries while a tag index shows counts, all
from one template. Because it is a template, paginated indexes look like the rest
of your site. This site's #link("/blog/")[blog] is paginated exactly this way.
