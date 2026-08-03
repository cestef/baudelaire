#let frontmatter = (
  title: "Asset pipeline",
  order: 1,
)
#import "/templates/theme.typ": callout

Everything under `assets/` is copied to the output untouched. Three switches turn
that copy into a build step.

```kdl
assets {
  minify #true
  bundle #true
  fingerprint #true
}
```

#table(
  columns: 4,
  align: (left, left, left, left),
  table.header([Key], [Type], [Default], [Does]),
  [`minify`],
  [flag],
  [`#false`],
  [Minifies CSS with #link("https://lightningcss.dev")[Lightning CSS], and JavaScript as part of bundling.],

  [`bundle`],
  [flag],
  [`#false`],
  [Bundles `.js`, `.mjs`, `.cjs`, `.jsx`, `.ts`, `.mts`, `.cts` and `.tsx` entry points with #link("https://rolldown.rs")[rolldown], resolving imports and shaking out dead code.],

  [`fingerprint`],
  [flag],
  [`#false`],
  [Puts a content hash in each output filename, and rewrites every reference to match.],

  [`tsconfig`],
  [path],
  [discovered],
  [The `tsconfig.json` TypeScript and JSX are transformed against. See #link(<typescript>)[TypeScript and JSX].],
)

Images sit in a nested `images` block and run on their own switches. See
#link("images.typ")[Images].

#callout(kind: "note")[
  JavaScript is only touched when `bundle` is on, because the bundler owns the
  whole JS step. `minify` alone minifies stylesheets and copies scripts verbatim.
]

== The directory

`paths { assets }` names the tree the pipeline reads, and the last segment is
also the URL prefix the results are served under:

```kdl
paths {
  assets "src/assets"
}
```

Files land at `/assets/...`, whatever subdirectory of the project they were
authored in. Point the pipeline somewhere else without touching any of the
switches above.

== How references are rewritten

Write the plain path in your template:

```typ
#html.elem("link", attrs: (rel: "stylesheet", href: "/assets/style.css"))
#html.elem("script", attrs: (type: "module", src: "/assets/main.js"))
```

With `fingerprint` on, `style.css` is written as `style.9f3c1a2b4d6e8f01.css` and
both attributes point at the hashed name after the build. The digest is 16 hex
digits of blake3 over the file's bytes.

The rewrite runs on the typed HTML tree, not on the serialized string, so it
can't corrupt markup. Stylesheets get the same treatment inside: `url()` and
`@import` references are rewritten to the hashed names too, and an imported sheet
is fingerprinted before the sheet that imports it.

Serve fingerprinted files with a far-future `Cache-Control`. The
#link("../ship/deploy.typ")[deploy] step reads `caching { immutable }` for
exactly those files.

#callout(kind: "tip")[
  A bundled entry can import the fingerprint map from the
  #link("../lookup/js-modules.typ")[`baudelaire:*` virtual modules], so client
  code names an asset by its logical path.
]

== TypeScript and JSX <typescript>

Entry points may be `.ts`, `.mts`, `.cts`, `.tsx` or `.jsx` as well as `.js`,
`.mjs` and `.cjs`. Types are stripped and JSX is transformed on the way through;
the output is always served as `.js`, and both spellings resolve:

```typ
#html.elem("script", attrs: (type: "module", src: "/assets/main.js"))
```

Nothing is type-checked. The bundler transforms, as esbuild and Vite do; run
`tsc --noEmit` in CI or a #link("hooks.typ")[`before` hook] for that. The
`baudelaire:*` modules an entry imports are typed by `baudelaire mirror`: see
#link("../lookup/js-modules.typ")[JS modules].

A `tsconfig.json` supplies the rest: `paths` aliases, `jsxImportSource`,
`experimentalDecorators`. One is discovered per script, walking up from the file
as `tsc` does. Pin one instead when the scripts sit far from it, or when more
than one is in reach:

```kdl
assets {
  bundle #true
  tsconfig "tsconfig.json"
}
```

The path is relative to the project root, and the build fails if nothing is
there. JSX defaults to the automatic runtime, so `jsxImportSource` (or a plain
`react` dependency) decides what the transformed code imports.

#callout(kind: "note")[
  A pinned `tsconfig.json` is not a watched source. Add it to
  `serve { include "tsconfig.json" }` for the dev server to rebuild when it
  changes.
]

== Partials

A script whose name starts with `_` is import-only. It is pulled into
whatever imports it and never emitted on its own:

```text
assets/
  main.js       -> /assets/main.js
  _search.js    -> nothing, inlined into main.js
```

== Bypassing the pipeline

Some files have to reach the output root byte for byte: a `robots.txt` override,
`.well-known/`, a `CNAME`, an `install.sh`. Put them under `static/`.

```kdl
paths {
  static "static"
}
```

The tree is mirrored into the output root: no minify, no bundle, no fingerprint,
no prefix. It is copied before pages and assets are written, so a generated file
at the same path wins. `static/` is the lowest-priority source.

== Running other tools

Tailwind, PostCSS, Pagefind and friends run as
#link("hooks.typ")[build hooks]. A `before` hook runs ahead of the pipeline, so
what it writes into `assets/` is minified and fingerprinted like anything else.

#callout(kind: "warn")[
  Minification needs the `css` feature and bundling needs `js`. A
  #link("../start/install.typ")[slim build] has neither: it warns and copies the
  files verbatim, and `fingerprint` turns itself off, since a verbatim stylesheet
  still names its assets by their pre-hash spelling.
]
