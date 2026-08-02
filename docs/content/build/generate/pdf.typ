#let frontmatter = (
  title: "PDFs",
  order: 11,
)
#import "/templates/theme.typ": callout

The same source, the other target. Your pages are already Typst, so a PDF is the
compiler laying the document out on paper instead of into a DOM.

```kdl
generate {
  pdf {
    pages { template "print.typ" }
  }
}
```

Every page gets a PDF beside its HTML, and its `<head>` grows a
`<link rel="alternate" type="application/pdf">` pointing at it. A page at
`/posts/hello/` gets `/posts/hello.pdf`, a sibling rather than an `index.pdf`
inside it, so a browser saves it under a name that means something.

Generated listings are skipped: a tag index is a table of contents for a site,
not a document anyone prints.

#callout(kind: "warn")[
  PDFs need the `pdf` cargo feature. The `slim` release drops it, and with it
  the PDF writer: a `pdf { }` block there writes nothing, links no page to a
  file it can't make, and warns. That covers both halves below. See
  #link("../../start/install.typ")[Install].
]

`baudelaire init --with pdf` writes the config block above, and every starter
shape scaffolds a `templates/print.typ` to begin from.

== The page template

A PDF is compiled as a *paged* document, so its template is separate from your
#link("../../write/templates.typ")[layout] for the same reason a
#link("cards.typ")[social card]'s is: `html.elem` draws nothing on this target,
and page layout, which does nothing in HTML, is the whole job here.

It's handed the same `page` dictionary your layout gets, built once for both, so
`page.frontmatter`, `page.date`, `page.reading`, `page.nav` and `page.strings`
mean the same thing on paper as on screen.

```typ
#let print(page, body) = {
  set std.page(paper: "a4", margin: 2cm)
  set text(font: "Libertinus Serif", size: 11pt)

  text(size: 24pt, weight: "bold", page.frontmatter.title)
  if page.date != none [ #v(0.5em) #text(fill: gray, page.date.display) ]
  v(1em)
  body
}
```

#callout(kind: "warn")[
  The `std.page` is deliberate. The first parameter is named `page` by
  convention, which shadows Typst's own `page` element, and a bare
  `set page(..)` then fails with `expected function, found dictionary`.
  `std.page` reaches the real one. Naming the parameter something else works
  too.
]

`print.typ` resolves against your `templates/` directory first and a
#link("../../write/theme-authoring.typ")[theme]'s second, like any other template.

== One document from many pages

The other half of the block binds pages together: a collection end to end, or
the whole site, as a single PDF.

```kdl
generate {
  pdf {
    bundle {
      template "book.typ"
      collections "guide"
      site #true
    }
  }
}
```

#table(
  columns: 4,
  align: (left, left, left, left),
  table.header([Key], [Type], [Default], [Does]),
  [`template`], [str], [`book.typ`], [The paged template the bundle is typeset with.],
  [`collections`], [str ..], [--], [Which collections to bind, one document each.],
  [`site`], [bool], [`#false`], [Bind the whole site as well.],
)

Each target is written as `/<target>.pdf`, so that config produces `/guide.pdf`
and `/site.pdf`, localized like every other per-language artifact
(`/fr/guide.pdf`). Pages are bound in the order the site already puts them: each
collection's own sort order. Generated listings are left out. A `bundle { }`
naming no target writes nothing, and the build says so.

The template is handed the document, then every page at once:

```typ
#let book(doc, entries) = {
  set std.page(paper: "a4", numbering: "1")
  set heading(numbering: "1.1")

  align(center + horizon, text(size: 26pt, weight: "bold", doc.title))
  pagebreak()
  outline(title: [Contents])

  for entry in entries {
    pagebreak(weak: true)
    heading(level: 1, entry.page.frontmatter.title)
    entry.body
  }
}
```

`doc` carries `id`, `title`, `lang`, `url`, `site`, `author` and `pages`, the
number of pages bound. Each `entry` carries the same `page` dictionary a layout
gets, plus its compiled `body`, so a contents list, running heads and continuous
numbering are the template's to decide.

The `book` and `docs` starter shapes scaffold a `templates/book.typ`.

#callout(kind: "note")[
  This is the paged counterpart of
  #link("../../ship/navigating.typ")[`navigation { standalone }`], which folds the
  same site into one *HTML* file. Nothing stops you emitting both.
]

== Determinism

The bytes are stable: the same page, and the same bundle, export the same file
every time. Typst stamps an export timestamp and a document identifier into a
PDF, and both default to the moment of export, which would give every build
different bytes for pages that hadn't changed and re-upload the whole site on
every #link("../../ship/deploy.typ")[deploy]. baudelaire pins them instead, to the
build's own date (what `sys.inputs.baudelaire.date` reports) and to the page's
permalink.

== Cost

A second compile per page, and unlike a card it lays the whole document out
rather than one fixed-size page. A bundle is one more compile again, of every
page it binds. Both are off unless you ask.

The #link("../incremental.typ")[incremental cache] covers it. A page that didn't
change keeps last build's PDF, and editing the paged template, or any module it
imports, re-exports the pages that read it. Deleting a PDF from the output
directory makes its page stale, so the file comes back on the next build.

A bundle belongs to no single page, so it gets a cache entry of its own: it's
re-exported when any page it binds changes, when one is added, removed or
reordered, and when its template (or anything that template imports) is edited.
