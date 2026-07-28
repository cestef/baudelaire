#let frontmatter = (
  order: 10,
  title: "Meta & images",
  tags: ("feature", "seo"),
)
#import "/templates/theme.typ": callout

Baudelaire enriches the HTML it emits: every page gets SEO and social meta tags
in its `<head>`, images load lazily, and raster images can be optimized at build
time. Meta tags live in the top-level `html` block; image handling in
`assets { images }`, because images go through the
#link("../assets/assets.typ")[asset pipeline] like anything else.

```kdl
html { meta #true }

assets {
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
/ Author: a `<meta name="author">` from a page's `author`, falling back to the
  site-wide `author` in config.
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
#link("../assets/assets.typ")[fingerprinted] URL and made absolute, so crawlers fetch the
cache-busted file.

== Images

The `assets { images }` block does four independent things.

/ #raw("lazy"): every `<img>` gains `loading="lazy"` (offscreen images defer
  until needed) and `decoding="async"` (decoding never blocks rendering). On by
  default; anything you set yourself is left as authored.
/ #raw("extract"): lift Typst-embedded images out of the page. Typst inlines an
  `#image("photo.png")` into the HTML as a base64 `data:` URI, which bloats the
  page and can't be cached. With `extract` (on by default), the bytes are
  written to a file under the asset URL and the `<img>` references it
  (`/assets/photo.png`), so the markup stays small and the image caches like any
  asset. Sizing (`#image("x", width: 50%)`) is preserved as inline CSS, so the
  output matches Typst's own. The original filename is kept; with
  `assets { fingerprint }` on, the served name carries a content hash. Set
  `extract #false` to keep Typst's inlining.
/ #raw("responsive"): pre-generate downscaled copies of each raster and emit a
  `srcset`, so a browser fetches the smallest size that fits its screen. Enabled
  by the block's presence; `widths` sets the target pixel widths (default
  `480 960 1440`), `quality` the JPEG re-encode quality (PNG stays lossless), and
  `sizes` an optional layout hint. A width at or above the source is skipped
  (never upscaled), and the source is always the largest candidate.
/ #raw("optimize"): a block of raster formats to shrink at build time. Name a
  format to enable it, with optional tuning. `png` is lossless (oxipng, `level`
  0–6 and `strip` none/safe/all); `jpeg` re-encodes at a `quality` (1–100).
  Extensions match leniently, so `jpeg` also covers `.jpg`. An optimizer never
  emits a file larger than the original.

```kdl
assets {
  images {
    extract #true
    responsive {
      widths 480 960 1440
      sizes "(min-width: 60rem) 640px, 100vw"
    }
    optimize {
      png level=4 strip="all"
      jpeg quality=75
    }
  }
}
```

#callout(kind: "note")[
  `extract` only touches `#image(..)` that names a project file. An image built
  from raw bytes, or one from a package (`@preview/..`), has no project file to
  reference, so it stays inline. `responsive` applies to any raster the pipeline
  sees, and pairs with the source-directory assets; `optimize` is opt-in per
  format.
]
