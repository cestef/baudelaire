#let frontmatter = (
  order: 6,
  title: "JS virtual modules",
  tags: ("feature", "assets"),
)
#import "/templates/theme.typ": callout

When you bundle JavaScript (`assets { bundle #true }`), baudelaire
serves the site's build data as `baudelaire:*` modules. Import one and rolldown
inlines the data into your bundle, so there is no runtime fetch and nothing to
keep in sync by hand.

```js
import site from "baudelaire:site";
import { url } from "baudelaire:assets";

document.title = site.title;
loadLogo(url("/assets/logo.png"));   // the fingerprinted name
```

Every module has a default export. When the data is an object, each key that is
a valid, non-reserved JavaScript identifier is also a named export, so
`import { title } from "baudelaire:site"` pulls in just that one; keys that
aren't legal identifiers stay reachable through the default export only.

== The modules

/ #raw("baudelaire:site"): `title`, `url`, `lang`, `author`, and `version`,
  the values templates also read from `sys.inputs.baudelaire`. See
  #link("../build/context.typ")[build context].
/ #raw("baudelaire:config"): your own constants, from the `client` block below.
/ #raw("baudelaire:assets"): the map from a request path to its fingerprinted
  URL, and a `url(path)` helper that returns the hashed name (or the path
  unchanged if it is not a known asset). Use it to point at a hashed
  #link("assets.typ")[asset] from script code.
/ #raw("baudelaire:pages"): the content pages, each as `{ url, title,
  collection, date, taxonomies }`.
/ #raw("baudelaire:sections"): the tree of content directories as
  `{ id, pages: [...], children: [...] }` nodes, what templates get as
  the `sections(lang)` Typst binding. `children` nests subdirectories, so recurse it for the whole
  tree.
/ #raw("baudelaire:taxonomies"): each taxonomy's terms mapped to the pages that
  carry them, e.g. `{ tags: { rust: [{ url, title }], .. } }`, for tag
  filtering or a term cloud.
/ #raw("baudelaire:feed"): the most recent dated pages as `{ url, title, date
  }`, newest first (capped at the feed `limit`), for a "latest posts" widget.
/ #raw("baudelaire:search"): the search-palette client. See
  #link("../discovery/search.typ")[search]. Pin an index shape with
  #raw("baudelaire:search/json") or #raw("baudelaire:search/inverted").
/ #raw("baudelaire:i18n"): the declared `languages` (`{ code, name }`) and their
  UI-string tables keyed by code, for a client-side language switcher. See
  #link("../content/i18n.typ")[internationalization].
/ #raw("baudelaire:spa"): the client-side navigation runtime, exporting
  `mountSpa(options)` and the lower-level `mountRouter`. See
  #link("../build/navigating.typ")[SPA and single-file export].

== Client constants

The `client` block passes values through to `baudelaire:config`. Any KDL scalar
works:

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

Use it for settings you would otherwise hard-code, or read from the environment
at runtime. The same constants reach templates at
`sys.inputs.baudelaire.client`, so server-side Typst and client-side JavaScript
read one source.

#callout(kind: "note")[
  A `baudelaire:assets` import sees every image, stylesheet, and copied asset,
  because scripts are bundled last, once the fingerprint map is done. One bundle
  cannot see another bundle's hashed name.
]
