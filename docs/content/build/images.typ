#let frontmatter = (
  title: "Images",
  order: 2,
)
#import "/templates/theme.typ": callout

Images ride the #link("assets.typ")[asset pipeline], plus four things of their
own in `assets { images }`.

```kdl
assets {
  images {
    lazy #true
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

#table(
  columns: 4,
  align: (left, left, left, left),
  table.header([Key], [Type], [Default], [Does]),
  [`lazy`],
  [flag],
  [`#true`],
  [Marks every `<img>` `loading="lazy"` and `decoding="async"`.],

  [`extract`],
  [flag],
  [`#true`],
  [Writes typst-embedded images out as files instead of inlining them.],

  [`responsive`],
  [block],
  [off],
  [Pre-generates width variants and emits a `srcset`.],

  [`optimize`],
  [block],
  [off],
  [Recompresses rasters at build time, one block per format.],
)

Only PNG and JPEG are processed. Anything else is copied through.

== Lazy loading

`lazy` is attribute-only: it fills `loading` and `decoding` on an `<img>` that
doesn't already set them, and touches no bytes. Anything you wrote yourself is
left as authored.

== Extraction

Typst inlines `#image("photo.png")` into the HTML as a base64 `data:` URI. That
bloats the page and the browser can't cache it separately. With `extract`, the
bytes are written under the asset URL and the `<img>` points at them:

```typ
#image("photo.png", width: 50%)
```

becomes an `<img src="/assets/photo.png">` with the sizing preserved as inline
CSS, so the output matches typst's own layout. The original filename is kept;
with `assets { fingerprint }` on, the served name carries a content hash like any
other asset.

#callout(kind: "note")[
  Extraction needs a project file to point at. An image built from raw bytes, or
  one that came from a `@preview` package, stays inline. So does everything when
  `html { embed }` is on, which inlines processed assets on purpose.
]

An extracted image is `optimize`d like any other, so a photo sitting beside its
page (the #link("../write/pages.typ")[page bundle] layout) is not the one
unrecompressed file on the site. `responsive` is the exception, and the one
reason to keep a shared image under `assets/`:

#table(
  columns: 3,
  align: (left, left, left),
  table.header([Where the file is], [`optimize`], [`responsive`]),
  [Under `assets/`], [yes], [yes],
  [Beside the page, extracted], [yes], [no],
)

A `srcset` names files that have to exist when the page naming them renders, and
an extracted image is only known to exist once that page *has* rendered. An
image in the asset tree is processed before any page compiles, so its variants
are there to offer. Referencing one from a page (`#image("/assets/photo.png")`)
gets the full pipeline and no second copy.

Set `extract #false` to keep typst's inlining.

== Responsive variants

The block's presence turns variants on. Each raster gets a downscaled copy per
width, and the `<img>` gets a `srcset` so the browser fetches the smallest one
that fits:

```kdl
assets {
  images {
    responsive {
      widths 480 960 1440
      quality 80
      sizes "(min-width: 60rem) 640px, 100vw"
    }
  }
}
```

#table(
  columns: 4,
  align: (left, left, left, left),
  table.header([Key], [Type], [Default], [Does]),
  [`widths`],
  [numbers],
  [`480 960 1440`],
  [The pixel widths to emit a variant at.],

  [`quality`],
  [number],
  [`80`],
  [Encoder quality for the generated variants, 1 to 100.],

  [`sizes`],
  [str],
  [--],
  [The `sizes` attribute put on every responsive image.],
)

A variant is named `photo-480.jpg`, the same splice a fingerprint uses. Variants
stay in the source format, so a JPEG source yields smaller JPEGs and a PNG stays
lossless, and each one goes through `optimize` like the file it was cut from. A
width at or above the source width is skipped, never upscaled, and the source
itself is always the largest candidate.

`sizes` has no default. A `w`-descriptor `srcset` already implies `100vw` to the
browser, so emitting it would cost bytes for nothing. Set it to your real content
width.

== Optimizing

Name a format to turn it on. The attributes are optional tuning.

```kdl
assets {
  images {
    optimize {
      png level=4 strip="all"
      jpeg quality=75
    }
  }
}
```

#table(
  columns: 4,
  align: (left, left, left, left),
  table.header([Key], [Type], [Default], [Does]),
  [`png level`],
  [number],
  [`2`],
  [#link("https://github.com/oxipng/oxipng")[oxipng] compression effort, 0 to 6. Higher is slower and smaller.],

  [`png strip`],
  [enum],
  [`safe`],
  [Which ancillary chunks to discard: `none`, `safe` or `all`.],

  [`jpeg quality`],
  [number],
  [`82`],
  [Re-encode quality, 1 to 100.],
)

PNG is lossless. JPEG is a re-encode, and it bakes in EXIF orientation so a
rotated photo stays rotated once the metadata is gone. Extensions match
leniently, so `jpeg` covers `.jpg` too. An optimizer never emits a file larger
than its input: if the re-encode grows, the original is kept.

== Elsewhere

- A social `image` in frontmatter is rewritten to its fingerprinted URL and made
  absolute. See #link("generate/meta.typ")[Meta tags & social].
- `lint { budget { images } }` weighs what a page references, not its `srcset`
  alternatives. See #link("linting.typ")[Linting & budgets].

#callout(kind: "warn")[
  `optimize` and `responsive` need the `images` feature. A
  #link("../start/install.typ")[slim build] warns and copies rasters through
  unchanged. `lazy` and `extract` are markup-only and work either way.
]
