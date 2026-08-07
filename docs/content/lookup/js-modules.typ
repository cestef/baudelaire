#let frontmatter = (
  title: "JS modules",
  order: 3,
)
#import "/templates/theme.typ": callout

Import the site's build data straight into your bundle. Rolldown inlines it, so
there is no runtime fetch and nothing to keep in sync by hand.

```js
import site from "baudelaire:site";
import { url } from "baudelaire:assets";

document.title = site.title;
loadLogo(url("/assets/logo.png"));  // the fingerprinted name
```

These are served to the bundler, so they need `assets { bundle #true }` and a
binary with the `js` feature. See the #link("../build/assets.typ")[asset
pipeline]. They are the JavaScript counterpart of the
#link("typst-modules.typ")[`@baudelaire/*` Typst modules], built from the same
data.

== TypeScript

Nothing on disk holds these modules, so an editor reads every import of one as
unknown. Every build writes the declarations out, and `baudelaire mirror` does
it on demand for a checkout that has not been built:

```sh
baudelaire mirror
```

Either way the file is `.baudelaire/generated/baudelaire.d.ts`. Put it on your
`include` list:

```json
{ "include": ["assets/**/*.ts", ".baudelaire/generated/baudelaire.d.ts"] }
```

`site`, `config` and `i18n` are typed from your own config, so `config.api` is
the type your `client { }` block gives it, not `unknown`. A build never reads
the file back, so a stale copy misleads an editor and changes no page.

== The modules

#table(
  columns: 3,
  align: (left, left, left),
  table.header([Specifier], [Exports], [Is]),
  [`baudelaire:site`], [default + named], [Site identity and baudelaire's version.],
  [`baudelaire:config`], [default + named], [Your own constants from the `client { }` block.],
  [`baudelaire:assets`], [default, `url`], [Request path to fingerprinted URL.],
  [`baudelaire:pages`], [default], [Every authored page as a row.],
  [`baudelaire:sections`], [default], [Section trees keyed by language code.],
  [`baudelaire:taxonomies`], [default], [Each taxonomy's terms mapped to pages.],
  [`baudelaire:feed`], [default], [The most recent dated pages.],
  [`baudelaire:i18n`], [`languages`, `strings`], [Declared languages and their UI strings.],
  [`baudelaire:search`], [`createSearch`, `mountSearch`], [The search client.],
  [`baudelaire:spa`], [`mountSpa`, `mountRouter`], [The client-side navigation runtime.],
)

Where a module's data is an object, every key that is a valid, non-reserved
JavaScript identifier is also a named export, so
`import { title } from "baudelaire:site"` pulls in that one and tree-shakes the
rest. Keys that aren't legal identifiers stay reachable through the default
export.

== site

The same values templates read from `sys.inputs.baudelaire`. See
#link("context.typ")[build metadata].

```js
import { version, title, url, lang, author, languages } from "baudelaire:site";
```

`title`, `url` and `author` are `null` when the config never set them.
`languages` is `{ code, name }` objects, default first, and is empty unless
#link("../write/i18n.typ")[i18n] is on.

== config

Build-time constants from the `client { }` block. Any KDL scalar works.

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

Use it for settings you would otherwise hard-code. The same constants reach
templates at `sys.inputs.baudelaire.client`, so server-side Typst and client-side
JavaScript read one source.

== assets

```js
import assets, { url } from "baudelaire:assets";

url("/assets/logo.png");  // "/assets/logo.a1b2c3d4.png"
url("/nope.png");         // "/nope.png"
```

`url(path)` returns the fingerprinted name, or the path unchanged when it is not
a known asset. The default export is the raw map.

#callout(kind: "note")[
  This sees every image, stylesheet, and copied asset, because scripts are
  bundled last, once the fingerprint map is done. One bundle cannot see another
  bundle's hashed name.
]

== pages

An array of rows, one per authored page, the same shape a generated listing hands
its template and the Typst `@baudelaire/pages` catalogue serves:
`url`, `label`, `collection`, `lang`, `date`, `display`, `note`, `description`,
`image`, `alt`, `author`, `taxonomies`, `extra`. Generated listings and the
not-found page are excluded.

```js
import pages from "baudelaire:pages";

const posts = pages.filter((p) => p.collection === "posts" && p.lang === "en");
```

== sections

The section trees, keyed by language code, each node
`{ id, pages: [{ url, title }], children: [...] }` per content directory. Exactly
what a page of that language gets as `page.sections`.

```js
import sections from "baudelaire:sections";

for (const node of sections.fr) walk(node);
```

`children` nests subdirectories, so recurse it for the whole tree.

== taxonomies

Each #link("../write/collections/taxonomies.typ")[taxonomy]'s terms mapped to the pages that
carry them, for tag filtering or a term cloud.

```js
import taxonomies from "baudelaire:taxonomies";

taxonomies.tags.rust;  // [{ url, title, lang }, ...]
```

== feed

The most recent dated pages as `{ url, title, lang, date }`, newest first, capped
at the #link("../build/generate/feeds.typ")[feed]'s configured `limit`. Every language is
included, each row tagged with its own, so one bundle serves the whole site.

== i18n

```js
import { languages, strings } from "baudelaire:i18n";

strings.fr.more;  // the fr UI string
```

`languages` is `{ code, name, dir }` objects, default first; `dir` is `ltr`
unless the config says otherwise. `strings` is keyed by code. For a language
switcher and localized UI text. See #link("../write/i18n.typ")[multiple
languages].

== search

The palette client, minus the auto-mount the standalone file carries: you decide
when it mounts and against what.

```js
import { mountSearch } from "baudelaire:search";

mountSearch({ placeholder: "Search the docs" });
```

`createSearch(url)` is the lower-level half: it fetches an index and resolves to
a `search(query, { limit })` function. The bare specifier follows the emitted
index; pin a shape with
`baudelaire:search/json` or `baudelaire:search/inverted`. See
#link("../build/generate/search.typ")[search].

== spa

```js
import { mountSpa } from "baudelaire:spa";

mountSpa({ select: "#content" });
```

Served whether or not `navigation { spa { } }` is set: importing it is itself the
opt-in, and the block's fields are only the defaults `mountSpa()` starts from.
`mountRouter` is the core underneath, for a site driving navigation itself. See
#link("../ship/navigating.typ")[SPA and single-file export].
