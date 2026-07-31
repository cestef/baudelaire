#let frontmatter = (
  order: 24,
  title: "PDFs",
  tags: ("feature", "typst", "output"),
  summary: "A PDF of every page, laid out from the same source by the same typesetter, beside the HTML.",
)
#import "/templates/theme.typ": callout

The same source, the other target. Your pages are already Typst, and Typst
already sets type on paper, so a PDF of a page is not an export step bolted on:
it is the compiler doing what it was written to do.

```kdl
generate {
  pdf {
    pages { template "print.typ" }
  }
}
```

Every page gets `/<permalink>.pdf` beside its HTML, and the page's `<head>`
grows a `<link rel="alternate" type="application/pdf">` pointing at it. A page
at `/posts/hello/` gets `/posts/hello.pdf`, so a browser saves it under a name
that means something rather than under `index.pdf`.

Generated listings are skipped: a tag index is a table of contents for a site,
not a document anyone prints.

== The template

A PDF is compiled as a *paged* document, so its template is separate from your
layout for exactly the reason a social card's is: `html.elem` draws nothing on
this target, and page layout, which does nothing in HTML, is the whole job here.

It is handed the same `page` dictionary your layout gets, built once for both,
so `page.frontmatter`, `page.date`, `page.reading`, `page.nav`, `page.strings`
and the rest mean the same thing on paper as on screen.

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

#callout(kind: "warning")[
  Note the `std.page`. The template's first parameter is named `page` by
  convention, which shadows Typst's own `page` element, and a bare
  `set page(..)` then fails with `expected function, found dictionary`. `std.page`
  reaches the real one. Naming the parameter something else works too.
]

Every starter shape scaffolds a `templates/print.typ` to begin from, and
`baudelaire init --with pdf` writes the config block above alongside it. A theme
may ship the template like any other, so `print.typ` resolves against your
`templates/` directory first and the theme's second.

== One document from many pages

The other half of the block binds pages *together*: a collection end to end, or
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

Every target is written as `/<target>.pdf`, so that config produces
`/guide.pdf` and `/site.pdf`, localized like every other per-language artifact
(`/fr/guide.pdf`). Pages are bound in the order the site already puts them: each
collection's own sort order. Generated listings are left out.

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

`doc` carries `id`, `title`, `lang`, `url`, `site`, `author` and the number of
pages bound. Each `entry` carries the same `page` dictionary a layout gets, plus
its compiled `body`, so a contents list, running heads and continuous numbering
are the template's to decide rather than something the generator guesses at.

The `book` and `docs` starter shapes scaffold a `templates/book.typ` to begin
from.

#callout(kind: "note")[
  This is the paged counterpart of
  #link("navigating.typ")[`navigation { standalone }`], which folds the same site
  into one *HTML* file. They are different artifacts for different readers: one
  is a site you can browse from a USB stick, the other is a document you can
  print or send. Nothing stops you emitting both.
]

A bundle belongs to no single page, so it has a cache entry of its own: it is
re-exported when any page it binds changes, when one is added, removed or
reordered, and when its template (or anything that template imports) is edited.
A build that changed none of those leaves the file alone.

== Determinism

The bytes are stable: the same page, and the same bundle, export the same file
every time. Typst
stamps an export timestamp and a document identifier into a PDF, and both
default to the moment of the export, which would make every build produce
different bytes for pages that had not changed and re-upload the whole site on
every deploy. Baudelaire pins them instead, to the build's own date (the one
`sys.inputs.baudelaire.date` reports) and to the page's permalink.

== Cost

A second compile per page, and unlike a card it lays the whole document out
rather than one fixed-size page. A bundle is one more compile again, of every
page it binds. Both are off unless you ask for them.

The incremental cache covers it exactly as it covers everything else: a page
that did not change keeps the PDF from the last build, and editing the paged
template, or any module it imports, re-exports the pages that read it. Deleting
a PDF from `dist` makes its page stale, so the file comes back on the next
build rather than staying missing forever.

== Without the exporter

The `slim` release drops the `pdf` feature, and with it the PDF writer. A
`pdf { }` block in that build writes nothing and points no page at a file it
cannot make, rather than filling your pages with links to 404s. That covers both
halves: no per-page PDF, and no bundle.
