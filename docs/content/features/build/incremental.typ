#let frontmatter = (
  order: 11,
  title: "Incremental builds",
  tags: ("feature", "performance"),
)
#import "/templates/theme.typ": callout

Edit one page in a thousand and Baudelaire recompiles one page. The rest are
served straight from cache, so a rebuild is measured in milliseconds and `serve`
reloads the browser almost the instant you save.

```sh
$ baudelaire build
 built 420 pages (0 cached)      # cold: everything compiled

$ baudelaire build               # after editing one post
 built 420 pages (419 cached)    # only that post recompiled
```

== How it decides

Each page is fingerprinted by two things: the exact source Typst compiles, and
every file that compilation actually read. Typst's import and data-loading are
tracked, so the dependency list is precise rather than guessed. A page is reused
only when its fingerprint and all of its dependencies are unchanged.

That makes invalidation sharp and predictable:

/ Edit a post: only that post rebuilds.
/ Edit a shared template or an imported module: every page that used it rebuilds,
  and nothing else.
/ Change `config.kdl`: the whole site rebuilds, since config can change any
  permalink.
/ New commit or new day: the whole site rebuilds. Build metadata (git SHA, build
  date) is a manifest-level input, not a per-page dependency, so any change to it
  invalidates every page at once, whether or not the page displays it.

== The cache

Rendered HTML lives in `.baudelaire/cache/objects/`, stored content-addressed: identical output
is kept once, and an unchanged page's markup is never rewritten, so the cache
stays small and writes stay cheap. `build` and `serve` share it, so switching
between them recompiles nothing.

#callout(kind: "tip")[
  Persist `.baudelaire/` between CI runs and deploys become near-instant too.
  `--no-cache` forces a clean rebuild; `baudelaire clean` removes it entirely.
]

#callout(kind: "note")[
  One consequence of content-hashed asset URLs: with `fingerprint` on, changing
  a stylesheet rewrites the `<link>` on every page, so every page rebuilds. Use
  `serve --profile dev` (fingerprinting off) for instant CSS iteration.
]
