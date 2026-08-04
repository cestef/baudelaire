#let frontmatter = (
  title: "Redirects",
  order: 12,
)
#import "/templates/theme.typ": callout

Moved a page? List its old paths in the new page's frontmatter and the old links keep working.

```typ
#let frontmatter = (
  title: "Configuration",
  redirect: ("/old/config/", "/setup/"),
)
```

Baudelaire writes a small HTML stub at each old path: a meta refresh, a canonical link, and a manual fallback anchor, all pointing at this page's URL. No server rules, no `.htaccess`, just static files that work on any host.

#callout(kind: "warn")[
  One old path still needs the trailing comma: `redirect: ("/setup/",)`. Without it the value is a string, not a list.
]

== Real 301s

A stub is a client-side round trip: the browser loads a page, reads the refresh, and asks again. Netlify and Cloudflare Pages both read a `_redirects` file from the publish directory instead, so on those hosts turn it on:

```kdl
generate {
  redirects #true
}
```

Every declared old path becomes one rule, pointing at the page that claimed it:

```text
/old/config/ /configure/overview/ 301
/setup/ /configure/overview/ 301
```

#callout(kind: "warn")[
  The stubs are *replaced*, not joined. Both hosts serve a static file in preference to a redirect rule, so a stub left at the old path would win and the 301 would never fire.
]

Leave it off for a host that reads no rule file. The stubs work anywhere, which is why they are the default. See #link("../../ship/deploy.typ")[deploying] for which host reads what.

=== If you write your own `_redirects`

A file in `static/` is published verbatim and wins the path, so a hand-written `_redirects` keeps the rules you wrote and the generated ones are not merged into it. Rather than dropping every declared `redirect` on the floor, the build writes the stubs instead and says so:

```text
⚠ `public/_redirects` is your own file, so redirects were written as stubs
  help: merge the generated rules into it by hand, or drop `generate { redirects }` and keep the stubs
```

The old paths keep working either way. To get real 301s for them, paste the rules into your own file.

== Collisions

Two pages claiming the same old path is a warning, not a race. The first claim keeps the path and the second is dropped, with both source files named in the message.

== Translated pages

An old path is localized like the page it forwards to. Copying a page's frontmatter into `config.fr.typ` carries the `redirect` list along, and that copy claims `/fr/setup/` rather than fighting the original for `/setup/`. See #link("../i18n.typ")[multiple languages].
