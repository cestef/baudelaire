#let frontmatter = (
  order: 4,
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

== Real 301s on a host that speaks them

A stub is a client-side round trip: the browser loads a page, reads a meta
refresh, and asks again. That costs a page load and passes link equity worse
than a redirect the server answers with. Netlify and Cloudflare Pages both read
a `_redirects` file from the publish directory, so:

```kdl
generate {
  redirects #true
}
```

writes one rule per old path instead, each pointing at the page that claimed
it:

```
/old/config/ /guide/config/ 301
/setup/ /guide/config/ 301
```

The stubs are *replaced*, not joined. Both hosts serve a static file in
preference to a redirect rule, so a stub left at the old path would win and the
301 would never fire. Leave this off for a host that reads no rule file: the
stubs work anywhere, which is why they are the default.
