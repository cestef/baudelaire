#let frontmatter = (
  title: "Social cards",
  order: 10,
)
#import "/templates/theme.typ": callout

The image a link to your site unfurls into, drawn per page from a Typst
template. No headless browser, no service: the typesetter is already in the
binary.

```kdl
generate {
  cards {
    template "card.typ"
    width 1200
    height 630
  }
}
```

The block's presence turns it on, and `cards #false` turns it off. Every page
without an `image` of its own gets
a PNG under `cards/` in the output directory, and its `og:image` and
`twitter:image` point at it. A page at `/posts/hello/` gets
`/cards/posts/hello.png`; the home page gets `/cards/index.png`.

A page that names its own `image` in
#link("../../write/frontmatter.typ")[frontmatter] keeps it and renders nothing.
Generated listings are skipped.

#callout(kind: "warn")[
  Cards need the `cards` cargo feature. The `slim` release drops it, and with it
  the PNG encoder: a `cards { }` block there renders nothing, points no page at
  an image it can't make, and warns. See #link("../../start/install.typ")[Install].
]

#table(
  columns: 4,
  align: (left, left, left, left),
  table.header([Key], [Type], [Default], [Does]),
  [`template`], [str], [`card.typ`], [The template each card is drawn with, under your templates directory.],
  [`width`], [int], [`1200`], [Card width in pixels.],
  [`height`], [int], [`630`], [Card height in pixels.],
)

`1200 x 630` is the size every unfurler crops to. The ceiling on either is 4096,
so a typo can't ask for a gigapixel rasterization.

== The template

A card is compiled to a *paged* document, not an HTML one. That's the whole
difference from a #link("../../write/templates.typ")[layout]: `html.elem` doesn't
exist here, and page layout does. Write it the way you'd write a poster.

```typ
#let card(data) = rect(width: 100%, height: 100%, fill: rgb("#fdfcfa"))[
  #set text(size: 64pt, weight: "bold")
  #pad(60pt)[
    #data.title
    #v(1fr)
    #text(size: 28pt, weight: "regular")[#data.site]
  ]
]
```

The function's name is the template's filename stem, so `card.typ` exports
`card`.

The page is already sized for you, at one point per pixel:

```typ
#set page(width: 1200pt, height: 630pt, margin: 0pt)
```

That runs before your template, so the config's `1200 x 630` is
`1200pt x 630pt` on the page and `1200 x 630` in the PNG. Set your own `page`
rule if you want a different shape.

`data` carries:

#table(
  columns: 2,
  align: (left, left),
  table.header([Field], [Is]),
  [`title`], [The page's title.],
  [`url`], [Its permalink.],
  [`lang`], [Its language code.],
  [`collection`], [The collection it belongs to.],
  [`site`], [The site name, in this page's language.],
  [`author`], [The site author for this language, or `none`.],
  [`date`], [Its date as an ISO-8601 string, or `none`.],
  [`taxonomies`], [Its terms, keyed by taxonomy.],
  [`frontmatter`], [Its whole frontmatter dictionary.],
)

#callout(kind: "warn")[
  A card is one image. A template whose content overflows onto a second page
  fails the build rather than silently shipping only the first: raise `height`,
  or fit the content.
]

== Cost

This is the most expensive thing a build can do per page: a second compile and a
rasterization each. That's why it's off unless you ask.

The #link("../incremental.typ")[incremental cache] covers it. A page that didn't
change keeps last build's card, and editing the card template re-renders every
card, since no page imports it and nothing else would notice.
