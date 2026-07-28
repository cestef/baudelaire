#let frontmatter = (
  order: 8,
  title: "Search",
  tags: ("feature", "search"),
)
#import "/templates/theme.typ": callout

Baudelaire builds a search index from your rendered pages, so search runs on a
static JSON file with no external service and no search-as-a-service account. It
also ships a ready-made command palette that reads that index: the search box on
this site (press `/` or `Ctrl` `K`) is exactly that, no custom code.

== Turn it on

```kdl
generate {
  search {
    formats "inverted"
    fields "title" "body" "tags"
    minimum 2
    stopwords "the" "a" "and"
  }
}
```

`fields` chooses what to index. `minimum` drops tokens shorter than N characters
and `stopwords` drops the listed words; both prune the `inverted` index only,
where tokenization happens at build time, and have no effect on the flat `json`
format, which keeps every document's full body for in-browser ranking. Only the
page's main content is indexed, not the site chrome, so a search for "install"
doesn't match every page's navigation.

== Two index shapes

/ #raw("json"): a flat list of `{ url, title, tags, body }` documents. Ranked in
  the browser, so hits keep their body text for snippets. Bring your own client
  or a library like #link("https://fusejs.io")[Fuse.js] or MiniSearch.
/ #raw("inverted"): a prebuilt `term to documents` index, tokenized at build
  time, so the client just looks terms up: small payload, no indexing in the
  browser (and no body, so no snippets).

Ask for both with `formats "json" "inverted"`.

== The command palette, for free

Add `client #true` and the build emits a complete, self-mounting command palette
per format. Load the one script and you have a working `Ctrl` `K` search box, with
snippets, highlighted matches, and full keyboard control, no markup or CSS to
write:

```html
<script type="module" src="/search.js"></script>
```

The palette ships only a minimal, neutral stylesheet, expressed entirely through
`.bd-*` classes and custom properties, so your own CSS can restyle it or
`mountSearch({ styles: false })` can turn the defaults off completely.

== Bundle it into your own JavaScript

If you already bundle JavaScript (`assets { bundle }`), import the same
client from the #raw("baudelaire:search") virtual module and rolldown inlines it,
tree-shakes it, and fingerprints it alongside your code, no extra network request:

```js
import { mountSearch } from "baudelaire:search";

mountSearch({ hotkey: "k", placeholder: "Search the docs" });
```

This is exactly how the palette on this site works. Use
#raw("baudelaire:search/json") or #raw("baudelaire:search/inverted") to pin a
format.

== Just the engine

Both entry points also export `createSearch`, the headless query function, if you
would rather build your own UI:

```js
import { createSearch } from "baudelaire:search";

const search = await createSearch();
for (const hit of search("clean urls", { limit: 8 })) {
  console.log(hit.url, hit.title);
}
```

#callout(kind: "note")[
  The client tokenizes queries the same way the index was built, so results stay
  consistent whichever entry point you reach for.
]
