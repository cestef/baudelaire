#let frontmatter = (
  title: "Meta tags & social",
  order: 9,
)
#import "/templates/theme.typ": callout

Every page gets a description, Open Graph, Twitter Card and canonical tags in
its `<head>`, drawn from its frontmatter and the site config.

```kdl
html {
  meta #true
  jsonld #true
}
```

`meta` is on by default. `jsonld` is not.

== What the site states

Two tags say something no single page can. Both live in the `meta` block, which is the flag above with settings behind it:

```kdl
html {
  meta {
    twitter "@example"
    image "/og.png"
  }
}
```

#table(
  columns: 3,
  align: (left, left, left),
  table.header([Key], [Default], [Does]),
  [`twitter`], [--], [The site's account, as `twitter:site`. Without it a card credits whoever posted the link and nobody else.],
  [`image`], [--], [The preview image for a page that names none and gets no generated card.],
)

`image` is only a fallback: a page's own `image` wins, and so does a
#link("cards.typ")[generated card]. It is resolved and made absolute the same
way an authored one is, so `/og.png` becomes a full URL under the site's `url`.
It carries no `og:image:alt`, because one picture standing in for every page is
not described by any one page's title. A page that wants an alt names its own
`image` and its own `alt`.

#callout(kind: "note")[
  Typst's HTML export owns the document `<head>`, so these tags cannot be written in a layout. baudelaire appends them to the parsed DOM instead, so they are set in config.
]

== What the tags read

```typ
#let frontmatter = (
  title: "Launch day",
  summary: "Everything new in this release.",
  image: "/assets/launch.png",
  alt: "The release notes, open in a browser",
  author: "Ada",
  date: datetime(year: 2026, month: 3, day: 1),
)
```

#table(
  columns: 3,
  align: (left, left, left),
  table.header([Key], [Falls back to], [Fills]),
  [`description`], [`summary`], [`<meta name="description">`, `og:description`, `twitter:description`.],
  [`image`], [a #link("cards.typ")[generated card], then `html { meta { image } }`], [`og:image`, `twitter:image`.],
  [`alt`], [the page title, for a generated card], [`og:image:alt`.],
  [`author`], [the site-wide `author`], [`<meta name="author">`, `article:author`.],
  [`date`], [--], [`og:type`, `article:published_time`.],
  [`updated`], [--], [`article:modified_time`, emitted only when it moved.],
)

`alt` describes the image for a reader who cannot see it. A generated card needs none: it draws the page title, so its `og:image:alt` is the title.

== What gets emitted

#table(
  columns: 2,
  align: (left, left),
  table.header([Group], [Tags]),
  [Document], [`description` and `author`.],
  [#link("https://ogp.me")[Open Graph]], [`og:type`, `og:title`, `og:description`, `og:url`, `og:site_name`, `og:locale`, `og:image`, `og:image:alt`.],
  [Article], [`article:published_time`, `article:modified_time`, `article:author`, one `article:tag` per taxonomy term. Dated pages only.],
  [Twitter Card], [`twitter:card`, `twitter:title`, `twitter:description`, `twitter:image`, and `twitter:site` when the site names an account.],
  [Link relations], [`canonical`, one `alternate` per translation plus `x-default`, one `alternate` per #link("feeds.typ")[feed format], the page's #link("pdf.typ")[PDF] when it has one, and the #link("manifest.typ")[web app manifest] when the site writes one.],
)

A dated page is an `article`, everything else a `website`. Several unfurlers
read `article:published_time` for the dateline. `twitter:card` is
`summary_large_image` when the page has an image, `summary` when it doesn't.

Only tags with data are emitted, so a minimal page still gets a valid set.

=== URLs

`og:url` and the canonical link appear only when you set a base `url` in the
config, the same precondition as #link("feeds.typ")[feeds and the sitemap].

An `image` naming a local asset is resolved to its
#link("../assets.typ")[fingerprinted] URL and then made absolute against `url`, so
a crawler fetches the cache-busted file. An already-absolute `http` value is
left as authored. See #link("../images.typ")[Images] for what the pipeline does to
the file itself.

=== Locale

`og:locale` comes from `lang`, in Open Graph's spelling: `fr-CA` becomes
`fr_CA`. A bare `fr` is passed through rather than given an invented territory,
since `fr_FR` would be wrong for a Belgian site. Declare the region in `lang` to
get the full form.

== Structured data

```kdl
html {
  jsonld #true
}
```

Adds a #link("https://json-ld.org")[JSON-LD] island to every page's `<head>`: an
`Article` where the page is dated, a `WebPage` otherwise, with `headline`,
`description`, `image`, `url`, `datePublished`, `dateModified`, `author` and
`keywords` from the taxonomy terms.

It is built from the same facts as the meta tags, so the two cannot disagree about a page.

#callout(kind: "note")[
  This is off by default, unlike the meta tags. Structured data is a claim made to a search engine about what the page is, so you opt in.
]
