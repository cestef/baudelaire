#let frontmatter = (
  title: "Meta & images",
  tags: ("feature", "seo"),
)
#import "/templates/theme.typ": callout

Baudelaire enriches the HTML it emits: every page gets SEO and social meta tags
in its `<head>`, images load lazily, and raster images can be optimized at build
time. Meta tags live in the `html` block; image handling in the `images` block.

```kdl
output {
  html { meta #true }
  images {
    lazy #true          // loading="lazy" + decoding="async"
    optimize {
      png                // lossless (oxipng)
      jpeg quality=82    // re-encode
    }
  }
}
```

== Meta tags

typst-html owns the document `<head>`, so these tags cannot be written in a
layout. Baudelaire appends them to every page instead, drawing from frontmatter
and the site config:

/ Description: from a page's `description`, falling back to `summary`.
/ #link("https://ogp.me")[OpenGraph]: `og:title`, `og:type` (a dated page is an
  `article`, everything else a `website`), `og:description`, `og:image`,
  `og:site_name`, `og:locale`, and, when a base `url` is set, `og:url`.
/ Twitter Card: `twitter:card` (a large image when the page has one),
  `twitter:title`, `twitter:description`, and `twitter:image`.
/ Canonical: a `<link rel="canonical">` when a base `url` is set.

Set the source fields in a page's frontmatter:

```typ
#let frontmatter = (
  title: "Launch day",
  summary: "Everything new in this release.",
  image: "/assets/launch.png",
  author: "Ada",
)
```

A root-relative `image` (or a page's `url`) is made absolute against the site
`url` so crawlers and social cards resolve it; an already-absolute URL is left
untouched. Only the tags whose data exists are emitted, so a minimal page still
gets a clean, valid set.

#callout(kind: "note")[
  The URL-absolute tags (`og:url`, canonical) appear only when you set a
  canonical `url` in the config, the same precondition as
  #link("feeds.typ")[feeds and sitemap].
]

A social `image` that points at a local asset is rewritten to its
#link("assets.typ")[fingerprinted] URL and made absolute, so crawlers fetch the
cache-busted file.

== Images

The `images` block does two independent things.

/ #raw("lazy"): every `<img>` gains `loading="lazy"` (offscreen images defer
  until needed) and `decoding="async"` (decoding never blocks rendering). On by
  default; anything you set yourself is left as authored.
/ #raw("optimize"): a block of raster formats to shrink at build time. Name a
  format to enable it, with optional tuning. `png` is lossless (oxipng, `level`
  0–6 and `strip` none/safe/all); `jpeg` re-encodes at a `quality` (1–100).
  Extensions match leniently, so `jpeg` also covers `.jpg`. An optimizer never
  emits a file larger than the original.

```kdl
images {
  optimize {
    png level=4 strip="all"
    jpeg quality=75
  }
}
```

#callout(kind: "note")[
  Optimization is opt-in per format: an empty or absent `optimize` block leaves
  every image untouched. Only formats you name are processed.
]
