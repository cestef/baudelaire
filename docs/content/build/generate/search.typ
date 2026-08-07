#let frontmatter = (
  title: "Search",
  order: 7,
)
#import "/templates/theme.typ": callout

A static search index built from your rendered pages, and a ready-made Ctrl-K
palette that reads it. No service, no account.

```kdl
generate {
  search {
    formats "json"
    fields "title" "body" "tags"
  }
}
```

That writes `search.json` beside the pages. One index per language, so a visitor
on `/fr/` searches `/fr/search.json` and never gets English hits.

By default only the page's `<main>` region is indexed. Site chrome (header,
sidebar, footer) is skipped, so a search for "install" doesn't match every
page's navigation.

A layout that binds its prose to something else names it, and one that keeps
chrome *inside* that region lists what to drop:

```kdl
html {
  region {
    element "article"     // the element whose contents are the page's prose
    ignore "nav" "aside"  // dropped wherever they appear inside it
  }
}
```

That lives under `html` and not under `search`, because it is a fact about the
markup the site emits: the #link("feeds.typ")[feed] carrying each entry in full
reads the same region, so both mean the same thing by "the page's prose".

Both are tag names, matched whole and case-insensitively; nesting is counted, so
an `<article>` inside an `<article>` does not end the region early. A page with
no such element counts whole, which is what a 404 or a landing page wants.
`element ""` counts every page whole. (A feed does not share that fallback; see
#link("feeds.typ")[feeds].)

Two things are always dropped and need no naming: `<script>` and `<style>`, and
anything marked `aria-hidden="true"`. The second is why a heading's
#link("../../write/pages.typ")[self link] never lands in the index as a stray
`#`, and it works for a theme's own decorative markup too.

== Keys

#table(
  columns: 4,
  align: (left, left, left, left),
  table.header([Key], [Type], [Default], [Does]),
  [`formats`], [`json` | `inverted`], [--], [Which index files to write. Naming none turns search off.],
  [`fields`], [`title` | `body` | `tags`], [all three], [What of each page goes into its document.],
  [`stopwords`], [str ..], [--], [Words left out of the `inverted` index.],
  [`minimum`], [int], [`2`], [The shortest word the `inverted` index keeps.],
  [`ui`], [bool], [`#false`], [Also emit the self-mounting palette script.],
)

== Two index shapes

`json` writes `search.json`: a flat list of `{ url, title, tags, body }`
documents, ranked in the browser. Hits keep their body text, so you get
snippets. Feed it to your own client or to a library like
#link("https://fusejs.io")[Fuse.js] or MiniSearch.

`inverted` writes `search.inverted.json`: `{ documents, postings }`, tokenized
at build time. The client looks terms up instead of indexing, so the payload is
small, but there is no body and so no snippets.

Ask for both with `formats "json" "inverted"`.

#callout(kind: "warn")[
  `stopwords` and `minimum` prune the `inverted` index only. Set either on a
  `json`-only site and the build warns that they did nothing.
]

== The palette, for free

```kdl
generate {
  search {
    formats "json"
    ui #true
  }
}
```

Each format gets a matching client: `search.js` for `json`,
`search.inverted.js` for `inverted`. Load one and you have a working palette,
with snippets, highlighted matches and full keyboard control:

```html
<script type="module" src="/search.js"></script>
```

Cmd/Ctrl-K opens it anywhere, and `/` opens it when you aren't typing in a
field. Any element carrying `data-search-open` becomes a trigger too.

The palette ships a minimal stylesheet expressed entirely through `.bd-*`
classes and custom properties, so your own CSS can restyle it.

== Bundle it into your own JavaScript

If you already bundle (`assets { bundle }`, see
#link("../assets.typ")[the asset pipeline]), import the same client from the
#raw("baudelaire:search") #link("../../lookup/js-modules.typ")[virtual module] and
rolldown inlines, tree-shakes and fingerprints it with the rest of your code.
No extra request, and no `ui #true` needed.

```js
import { mountSearch } from "baudelaire:search";

mountSearch({ styles: false, placeholder: "Search the docs" });
```

That is exactly how the palette on this site works. The bare specifier follows
the index you configured; use #raw("baudelaire:search/json") or
#raw("baudelaire:search/inverted") to pin a format.

#table(
  columns: 3,
  align: (left, left, left),
  table.header([Option], [Default], [Does]),
  [`url`], [the emitted index], [Fetch the index from somewhere else.],
  [`limit`], [`12`], [How many hits to show.],
  [`placeholder`], [`Search`], [The input's placeholder text.],
  [`hotkey`], [`/`], [The extra key that opens it. Cmd/Ctrl-K is always bound.],
  [`styles`], [`true`], [Inject the default stylesheet. `false` opts out entirely.],
)

`mountSearch` is idempotent: a second call returns the first instance, so an
import and the auto-mount can coexist.

== The engine alone

Both entry points also export `createSearch`, the headless query function, if
you'd rather build your own UI:

```js
import { createSearch } from "baudelaire:search";

const search = await createSearch();
for (const hit of search("clean urls", { limit: 8 })) {
  console.log(hit.url, hit.title);
}
```

The bundled client points at the default language's index. Pass another
language's URL to `createSearch` to search it instead.

#callout(kind: "note")[
  The client tokenizes a query exactly the way the index was tokenized, so
  results stay consistent whichever entry point you reach for.
]
