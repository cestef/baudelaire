#let frontmatter = (
  title: "Incremental builds",
  order: 4,
)
#import "/templates/theme.typ": callout

Edit one page in a thousand and one page recompiles. It's on by default.

```sh
$ baudelaire build
 built 420 pages in 3.1s

$ baudelaire build          # after editing one post
 built 420 pages in 40ms
   419 cached
```

== What invalidates what

A page is fingerprinted by the exact typst source it compiles and by every file
that compilation actually read. Typst tracks its own imports and data loads, so
the dependency list is measured, not guessed. A page is reused only when its
fingerprint and all of its dependencies are unchanged.

#table(
  columns: 2,
  align: (left, left),
  table.header([You change], [What rebuilds]),
  [A post], [That post.],
  [A shared template or an imported module], [Every page that used it.],
  [An asset a page links], [Every page that links it, with the new URL.],
  [`config.kdl`], [The whole site: config can move any permalink.],
  [A build value, like the git hash], [Only the pages that read that value.],
)

That last row is per-field. A page printing `sys.inputs.baudelaire.git.hash`
rebuilds on a commit; one that only reads `.version` does not. See
#link("../lookup/context.typ")[Build metadata].

Render-side inputs are tracked the same way. A page records the permalinks its
links resolved to, the variants its images matched, and the assets its references
named, including the ones that were absent, so a reference to a page you haven't
written yet invalidates the moment you write it.

== The cache

```text
.baudelaire/cache/
  manifest.json          # config fingerprint, per-page metadata, dependency edges
  objects/ab/abcd...     # rendered HTML, content-addressed
```

The manifest holds metadata only, so a warm start parses a small JSON file
instead of every page's markup. Identical output is stored once, and an unchanged
blob is never rewritten. `build` and `serve` share the cache, so switching
between them recompiles nothing.

```kdl
cache {
  dir ".baudelaire/cache"
  incremental #true
}
```

`cache { incremental #false }` makes every build a cold one.

== Forcing a rebuild

```sh
$ baudelaire build --no-cache     # ignore the cache for this run
$ baudelaire clean --cache        # delete it
$ baudelaire clean --all          # and the output directory with it
```

`--no-cache` still writes the next manifest, so the following build is warm
again.

#callout(kind: "tip")[
  Persist `.baudelaire/` between CI runs and a deploy build is near-instant too.
]

#callout(kind: "note")[
  With `assets { fingerprint }` on, changing a stylesheet rewrites the `<link>`
  on every page, so every page rebuilds. Turn fingerprinting off in a `dev`
  #link("../configure/profiles.typ")[profile] for instant CSS iteration.
]

== What a cache hit skips

A page's HTML is written on every build, hit or miss, because the cache holds the
markup. A sidecar (#link("generate/cards.typ")[social card], #link("generate/pdf.typ")[PDF]) is
not: only the build that compiles a page draws one. Delete one by hand and the
page counts as stale, which is what redraws it.

#link("hooks.typ")[Hooks] run on every build, cache hit or not.
#link("linting.typ")[Lints and budgets] do too: each page replays the findings it
recorded, and the bytes are weighed fresh from what this build emitted, so a
fatter stylesheet fails the pages that load it without any of them changing.
