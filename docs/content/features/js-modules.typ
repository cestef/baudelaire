#let frontmatter = (
  order: 3,
  title: "JS virtual modules",
  tags: ("feature", "assets"),
)
#import "/templates/theme.typ": callout

When you bundle JavaScript (`output { assets { bundle #true } }`), baudelaire
exposes the site's build data as `baudelaire:*` virtual modules. Import one and
rolldown inlines the data straight into your bundle — tree-shaken, minified, and
fingerprinted like your own code. No `fetch`, no globals, no separate file to
keep in sync.

```js
import site from "baudelaire:site";
import { url } from "baudelaire:assets";

document.title = site.title;
loadLogo(url("/assets/logo.png"));   // resolves to the hashed name
```

Each module offers a default export and, where the data is an object, a named
export per key so `import { title } from "baudelaire:site"` shakes out the rest.

== The modules

/ #raw("baudelaire:site"): Site identity and build version — `title`, `url`,
  `lang`, `author`, `version`. The same values templates read from
  `sys.inputs.baudelaire`. See #link("context.typ")[build context].
/ #raw("baudelaire:config"): Your own build-time constants from the `client { }`
  config block (below).
/ #raw("baudelaire:assets"): The request → fingerprinted-URL map, plus a
  `url(path)` helper that returns the hashed name (or the input unchanged). Lets
  client JS reference a hashed #link("assets.typ")[asset] the way CSS and HTML
  already do.
/ #raw("baudelaire:pages"): The authored content pages as
  `{ url, title, collection, date, taxonomies }` — for related-posts, client
  nav, or prefetch.
/ #raw("baudelaire:sections"): Collections grouped as `{ id, pages: [{ url,
  title }] }`, the same set templates get as `page.sections` — for menus and
  command palettes.
/ #raw("baudelaire:search"): baudelaire's own search-palette client. See
  #link("search.typ")[search].

== Client constants

The `client` block passes plain values through to `baudelaire:config`. Any KDL
scalar works — string, number, boolean:

```kdl
client {
  analytics "https://plausible.io"
  revalidate 3600
  beta #false
}
```

```js
import { analytics, revalidate } from "baudelaire:config";
```

It is baudelaire's answer to a bundler's `define`: settings that belong at build
time, resolved once, with no runtime lookup.

#callout(kind: "note")[
  A `baudelaire:assets` import sees every image, stylesheet, and copied asset —
  scripts are bundled last, after the fingerprint map is complete. One bundle
  doesn't see another bundle's hashed name, which is rarely what you want anyway.
]
