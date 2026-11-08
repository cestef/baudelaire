#let frontmatter = (
  title: "TypeScript and JSX",
  order: 3,
)
#import "/templates/theme.typ": callout

Entry points may be `.ts`, `.mts`, `.cts`, `.tsx` or `.jsx` as well as `.js`,
`.mjs` and `.cjs`. Types are stripped and JSX is transformed on the way through;
the output is always served as `.js`, and both spellings resolve:

```typ
#html.elem("script", attrs: (type: "module", src: "/assets/main.js"))
```

Nothing is type-checked. The bundler only transforms, as esbuild and Vite do; run
`tsc --noEmit` in CI or a #link("../hooks.typ")[`before` hook] for that. The
`baudelaire:*` modules an entry imports are typed by `baudelaire mirror`: see
#link("../../lookup/js-modules.typ")[JS modules].

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
