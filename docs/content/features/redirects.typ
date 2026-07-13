#let frontmatter = (
  order: 8,
  title: "Redirects",
  tags: ("feature",),
)

When you move a page, leave a redirect behind so old links keep working. List
the old paths in a page's frontmatter:

```typ
#let frontmatter = (
  title: "Configuration",
  redirect: ("/old/config/", "/setup/"),
)
```

Baudelaire writes a small HTML stub at each old path that forwards to this page's
URL. No server rules, no `.htaccess`, just static files that work on any host.

This pairs well with #link("taxonomies.typ")[taxonomy] and permalink changes:
rename a slug, add the previous URL as a redirect, and nothing 404s.
