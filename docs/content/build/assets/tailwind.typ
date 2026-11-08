#let frontmatter = (
  title: "Tailwind",
  order: 2,
)
#import "/templates/theme.typ": callout

A `tailwind { }` block generates a utility stylesheet from the class names the
site is written with, using
#link("https://gitlab.com/encre-org/encre-css")[encre-css].

```kdl
assets {
  tailwind { }
}
```

```typ
#html.elem("p", attrs: (class: "flex gap-2 text-sm"), [hello])
```

That is the whole of it. The build links the sheet from every page itself, so no
template names a file, and the `<link>` it writes is an ordinary reference:
minified, fingerprinted, digested for `integrity` and inlinable like any other
asset. Only the classes the site actually writes get rules. Nothing is
downloaded, no `node_modules`, no watcher.

#table(
  columns: 4,
  align: (left, left, left, left),
  table.header([Key], [Type], [Default], [Does]),
  [`path`],
  [asset path],
  [`tailwind.css`],
  [Where the sheet is served from, relative to the asset root. Relative and inside the tree, or the build refuses it.],

  [`scan`],
  [list],
  [`content` and `templates`],
  [The trees and files read to find class names. A directory is read whole, a file on its own.],

  [`config`],
  [path],
  [none],
  [An #link("https://encre-css.pages.dev/docs/configuration/")[encre-css config] (TOML): theme, safelist, shortcuts, preflight.],

  [`preflight`],
  [flag],
  [`#true`],
  [Whether the sheet opens with the reset rules Tailwind puts in front of its utilities.],
)

By default the content and template trees are read, and only the `.typ` and
`.md` files in them: those are the two languages a page can be written in, and a
photograph colocated with a post is not read on every build. Name `scan`
yourself and everything under what you named is read, whatever it is called.

```kdl
assets {
  tailwind {
    scan "content" "templates" "themes/plume/templates" "assets/app.js"
  }
}
```

#callout(kind: "note")[
  A class name has to appear *literally* somewhere scanned. A class assembled at
  runtime (`"text-" + size`) is not a class name anyone can find, in this
  generator or in Tailwind's own: put the whole names in a `safelist` in the
  encre-css config.
]

#callout(kind: "note")[
  A tree named by `scan` outside `content` and `templates` is not watched by
  `baudelaire serve`. Add it to `serve { include }` to rebuild when it changes.
]

```kdl
assets {
  tailwind {
    path "css/utilities.css"      // -> /assets/css/utilities.css
  }
}
```

A site or theme shipping its own file at that path keeps it: the generated one
steps aside, as every generated asset does, and the pages go on linking whatever
is served there.
