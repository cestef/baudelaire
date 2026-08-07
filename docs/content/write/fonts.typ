#let frontmatter = (
  title: "Fonts",
  order: 8,
)
#import "/templates/theme.typ": callout

Typst resolves a glyph against the faces a build can see. By default those are its
own bundled ones and whatever is installed on the machine, which makes the output
depend on the machine: a page typeset in a face you have installed renders in a
fallback on CI, out of a build that reported success.

Ship the faces with the site instead:

```kdl
typst {
  fonts {
    paths "assets/fonts"
    system #false
  }
}
```

#table(
  columns: 3,
  align: (left, left, left),
  table.header([Key], [Default], [Does]),
  [`paths`], [none], [Directories scanned recursively, relative to the project root.],
  [`system`], [on], [Also use the fonts installed on the machine.],
)

Search order runs from most specific to least: Typst's bundled faces, then your
`paths`, then the system's. A face you ship therefore wins over a same-named one
that happens to be installed, which is the point of shipping it.

`system #false` is what makes the build reproducible. With it off, a page can only
resolve to a bundled face or one in the project, so the same commit typesets the
same bytes on your laptop and on CI.

#callout(kind: "note")[
  A directory that is not there is an error before anything compiles. Scanning one
  yields no faces and says nothing, so a typo would otherwise show up only as a
  page silently typeset in a fallback.
]

The faces under `paths` are build inputs, so replacing one rebuilds the site: a
new cut of a face reaches the pages, the social cards and the PDFs together. The
contents are what count, not the timestamps, so a fresh checkout on CI is not a
cold build. A site that names no directory pays nothing for this.

A face is used by naming it in Typst, as usual. The config only decides what can
be found:

```typ
#set text(font: "Inter")
```

#callout(kind: "warn")[
  Putting fonts under `assets/` means the pipeline also copies them to the output
  and a browser can fetch them, which is what you want for a webfont. A face you
  only typeset with (a PDF sidecar, a social card) is better kept outside the
  asset tree, so it is not published.
]
