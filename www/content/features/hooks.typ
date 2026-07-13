#let frontmatter = (
  order: 3,
  title: "Build hooks",
  tags: ("feature", "assets"),
)
#import "/templates/theme.typ": callout

Baudelaire embeds its own CSS minifier and JavaScript bundler, but sometimes you
want a tool it doesn't ship: Tailwind, PostCSS, Pagefind, an image optimizer.
Hooks run external commands around the build.

```kdl
hooks {
  before "tailwindcss -i assets/app.css -o assets/style.css --minify"
  after  "pagefind --site public"
}
```

- `before` runs *ahead of the asset pipeline*, so anything it writes into
  `assets/` is minified and fingerprinted like a first-class asset.
- `after` runs once the finished site is in `dist` (deploy scripts,
  post-processors).

Commands run through your shell in the project root and stream their output; a
non-zero exit fails the build.

== Tailwind, staying single-binary

Tailwind v4 ships a standalone CLI: one binary, no Node. Point a `before` hook at
it and let Baudelaire fingerprint the result:

```kdl
hooks {
  before "tailwindcss -i assets/app.css -o assets/style.css --minify"
}
serve {
  exclude "assets/style.css"
}
```

#callout(kind: "note")[
  In `serve`, a hook that writes into a watched directory would retrigger the
  watcher forever. List its output under `serve { exclude "..." }` (a wax glob)
  to break the loop, and use `serve { include "..." }` to watch sources that live
  outside content, templates, and assets.
]

This is how Baudelaire stays small: a fast native core for the common path, and a
clean escape hatch to the wider ecosystem when you need it.
