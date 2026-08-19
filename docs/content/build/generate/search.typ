#let frontmatter = (
  title: "Search",
  order: 7,
)
#import "/templates/theme.typ": callout

A static search index built from your rendered pages, and a ready-made Ctrl-K
palette that reads it. No service, no account.

```kdl
generate {
  search { }
}
```

That writes `search.json` beside the pages. The block's presence turns search
on; `search #false` turns it off again. One index per language, so a visitor on
`/fr/` searches `/fr/search.json` and never gets English hits.

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
  [`index`], [`terms` | `documents`], [`terms`], [Who builds the postings.],
  [`fields`], [block], [see below], [What a match in each part of a page is worth.],
  [`stopwords`], [str ..], [--], [Words left out of the index.],
  [`minimum`], [int], [`2`], [The shortest word the index keeps.],
  [`snippet`], [int], [`240`], [Characters of context a hit shows. `0` shows none.],
  [`ui`], [block], [off], [Ship the generated palette.],
)

== Weights, not a field list

Every key of `fields` is what a match in that part of a page is worth. A field
at `0` is left out of the index entirely, so one block says both what is indexed
and how it ranks:

```kdl
generate {
  search {
    fields {
      title 5   // the default
      tags 3
      body 1
    }
  }
}
```

`fields { tags 0 }` indexes titles and prose and ignores taxonomy terms. A hit
still shows its title whatever the title weight is: weights decide ranking, not
what a result looks like.

== Two index shapes

Both shapes tokenize, rank and snippet identically. They differ only in who
builds the postings, which is a payload trade and nothing else.

`index "terms"` prebuilds them here: `{ terms, postings }` beside each hit's
snippet. The browser looks a term up instead of reading every page's prose, so
the payload stays small on a large site.

`index "documents"` ships each page's prose whole and the client indexes it on
load, under the weights, stopwords and minimum the index header carries. Larger,
but it is the shape any other client library reads, so you can feed it to
#link("https://fusejs.io")[Fuse.js] or MiniSearch instead of the bundled engine.

Either way a query matches whole words, and the word still being typed matches
by prefix: "conf" finds "configuration" before you finish the word.

== The palette, for free

```kdl
generate {
  search {
    ui {
      hotkey "/"
      placeholder "Search the docs"
      limit 12
      styles #true
    }
  }
}
```

The `ui` block writes `/search.js`, one client for the whole site whatever
languages it has. Load it and you have a working palette, with snippets,
highlighted matches and full keyboard control:

```html
<script type="module" src="/search.js"></script>
```

Cmd/Ctrl-K opens it anywhere, and `hotkey` opens it when you aren't typing in a
field. Any element carrying `data-search-open` becomes a trigger too.

The palette ships a minimal stylesheet expressed entirely through `.bd-*`
classes and custom properties, so your own CSS can restyle it. `styles #false`
leaves the look entirely to you.

== Bundle it into your own JavaScript

If you already bundle (`assets { bundle }`, see
#link("../assets.typ")[the asset pipeline]), import the same client from the
#raw("baudelaire:search") #link("../../lookup/js-modules.typ")[virtual module] and
rolldown inlines, tree-shakes and fingerprints it with the rest of your code.
No extra request, and no `ui` block needed.

```js
import { mountSearch } from "baudelaire:search";

mountSearch({ styles: false, placeholder: "Search the docs" });
```

That is exactly how the palette on this site works. Every option defaults to
what `ui { }` configured, so a call overrides only what it names.

#table(
  columns: 3,
  align: (left, left, left),
  table.header([Option], [Default], [Does]),
  [`url`], [this page's language], [Fetch the index from somewhere else.],
  [`limit`], [`ui { limit }`], [How many hits to show.],
  [`placeholder`], [`ui { placeholder }`], [The input's placeholder text.],
  [`hotkey`], [`ui { hotkey }`], [The extra key that opens it. Cmd/Ctrl-K is always bound.],
  [`styles`], [`ui { styles }`], [Inject the default stylesheet.],
)

`mountSearch` is idempotent: a second call returns the first instance, so an
import and the auto-mount can coexist.

== The engine alone

The client also exports `createSearch`, the headless query function, if you'd
rather build your own UI:

```js
import { createSearch } from "baudelaire:search";

const search = await createSearch();
for (const hit of search("clean urls", { limit: 8 })) {
  console.log(hit.url, hit.title, hit.text);
}
```

It picks the index by the page's own `<html lang>`, so the same bundle searches
the right language on every page. Pass a URL to search another one.

#callout(kind: "note")[
  The client tokenizes a query exactly the way the index was tokenized, so
  results stay consistent whichever shape and whichever entry point you reach
  for.
]
