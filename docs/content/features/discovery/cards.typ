#let frontmatter = (
  order: 16,
  title: "Social cards",
  tags: ("feature", "images", "meta"),
)
#import "/templates/theme.typ": callout

The image a link to your site unfurls into, drawn per page from a Typst
template. No headless browser, no image editor, no service: the typesetter is
already in the binary, so a card is one more thing it lays out.

```kdl
output {
  cards {
    template "card.typ"
    width 1200
    height 630
  }
}
```

Every page without an `image` of its own gets `dist/cards/<permalink>.png`, and
its `og:image` and `twitter:image` point at it. A page that names its own image
keeps it and renders nothing.

== The template

A card is compiled to a *paged* document, not an HTML one. That is the whole
difference from a layout: `html.elem` does not exist here, and page layout does.
Write it the way you would write a poster.

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

The page is already sized for you: `width` and `height` are set before your
template runs, at one point per pixel, so `1200 x 630` in config is `1200pt x
630pt` on the page and `1200 x 630` in the PNG. Set your own `page` rule if you
want a different shape.

`data` carries `title`, `url`, `lang`, `collection`, `site`, `author`, `date`,
`taxonomies`, and the page's whole `frontmatter`, so a card can show anything the
page knows about itself.

#callout(kind: "warning")[
  A card is one image. A template whose content overflows onto a second page
  fails the build rather than silently shipping only the first: raise `height`,
  or fit the content.
]

== Cost

This is the most expensive thing a build can do per page: a second compile and a
rasterization each. It is worth knowing what that buys, so it is off unless you
ask for it.

The incremental cache covers it. A page that did not change keeps the card from
the last build, and editing the card template re-renders every card, since no
page imports it and nothing else would notice.

== Without the rasterizer

The `slim` release drops the `cards` feature, and with it the PNG encoder. A
`cards { }` block in that build renders nothing and points no page at an image
it cannot make, rather than filling your `og:image` tags with 404s.
