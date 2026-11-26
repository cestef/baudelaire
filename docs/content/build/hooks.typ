#let frontmatter = (
  title: "Build hooks",
  order: 3,
)
#import "/templates/theme.typ": callout

Run external commands around the build: Tailwind, PostCSS, Pagefind, an image
optimizer, a deploy script.

```kdl
hooks {
  before "tailwindcss -i assets/_app.css -o assets/style.css --minify"
  after "pagefind --site public"
}
```

#table(
  columns: 3,
  align: (left, left, left),
  table.header([Key], [Type], [Does]),
  [`before`],
  [str ..],
  [Commands run ahead of the asset pipeline, so what they generate is picked up.],

  [`after`],
  [str ..],
  [Commands run once the output directory is written.],
)

Each argument is one whole command line, and they run in the order written.

== How they run

Hooks run through the system shell (`sh -c` on Unix, `cmd /C` on Windows), in the
project root. The directory you started `baudelaire` from does not matter: a
hook's relative paths always land in the site being built.

Output streams straight to your terminal under a dimmed `$ command` line. A non-zero exit fails the build.

== Feeding the pipeline

`before` runs first, ahead of everything that reads a source tree. Anything it
writes into `assets/` is minified, bundled and fingerprinted like a file you
checked in, a file it writes into `static/` is published by the same build, and a
page it writes into `content/` is built.

```kdl
hooks {
  before "tailwindcss -i assets/_app.css -o assets/style.css --minify"
}
assets {
  minify #true
  fingerprint #true
}
serve {
  exclude "assets/style.css"
}
```

#callout(kind: "warn")[
  Under `serve`, a hook writing into a watched directory retriggers the watcher
  forever. List its output under `serve { exclude }` to break the loop.
]

`exclude` and `include` are #link("https://docs.rs/wax")[wax] globs relative to
the project root. `include` does the opposite: it watches sources that live
outside content, templates and assets. See
#link("preview.typ")[Dev server & preview].

== After the build

`after` sees the finished output directory:

```kdl
hooks {
  after "pagefind --site public" "gzip -kf public/index.html"
}
```

Several commands go on one line, one string each. Writing `after` twice replaces
the list rather than extending it, the way every config list behaves.

Nothing re-reads what an `after` hook writes, so files it creates are
outside the #link("incremental.typ")[build cache] and outside
#link("linting.typ")[budgets]. To have a generated file weighed and
fingerprinted, write it in a `before` hook instead.
