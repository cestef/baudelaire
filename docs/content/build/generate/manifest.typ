#let frontmatter = (
  title: "Web app manifest",
  order: 12,
)
#import "/templates/theme.typ": callout

A `manifest.webmanifest` is what a browser reads when a visitor installs the
site to a home screen: what to call it, what to launch, what to paint before the
first page renders. The block's presence writes it, and `manifest #false` stops
it.

```kdl
site "Baudelaire"

generate {
  manifest {
    short "Baudelaire"
    description "A Typst-native static site generator"
    display "standalone"
    theme "#101014"
    background "#ffffff"
    icons {
      "/icons/app-192.png" size=192
      "/icons/app-512.png" size=512
      "/icons/app-512-maskable.png" size=512 purpose="maskable"
    }
  }
}
```

Every page then carries `<link rel="manifest">`, plus a `theme-color` meta tag
when you set `theme`. Without the link a browser never looks for the file.

== Keys

#table(
  columns: 4,
  align: (left, left, left, left),
  table.header([Key], [Type], [Default], [Does]),
  [`name`], [string], [the site title], [The installed app's name.],
  [`short`], [string], [--], [What a launcher shows when the full name doesn't fit.],
  [`description`], [string], [--], [One line, shown by an install prompt.],
  [`display`], [`standalone` | `fullscreen` | `minimal` | `browser`], [`standalone`], [How the app is presented. `minimal` writes `minimal-ui`.],
  [`theme`], [string], [--], [CSS colour of the browser UI, and of every page's `theme-color`.],
  [`background`], [string], [--], [CSS colour painted before the first render.],
  [`start`], [string], [the language's root], [Where launching the app lands. Localized per language.],
  [`scope`], [string], [the language's root], [The URLs the app covers. Localized per language.],
  [`icons`], [block], [--], [One line per icon, named by the path it is served from.],
)

What the build already knows it fills in: the name from `site`, `start` and
`scope` from where that language's site begins, an icon's media type from its
extension. An authored `start "/home/"` is localized like the default it
replaces, so the French app launches into `/fr/home/` and stays inside the
French scope. Under a base path (`url "https://example.com/docs"`) every URL it
writes carries the prefix, because a browser resolves none of them against the
page the manifest was linked from.

== Icons

```kdl
icons {
  "/icons/app-192.png" size=192
  "/icons/app.svg"
  "/icons/badge.png" size=96 purpose="monochrome"
}
```

The node name is the path, exactly as the browser will request it: put the files
in `static/` and they're served verbatim. `size` is the square edge in pixels,
and leaving it out means the image scales to any size, which is the honest
answer for an SVG.

`purpose` is `any` (the default, and then left unwritten), `maskable` for an
image safe to crop to the platform's icon shape, or `monochrome` for a
single-colour glyph the platform recolours.

#callout(kind: "warn")[
  A manifest with no icons is written and linked, and nothing will ever offer to
  install it. The build says so (`baudelaire::manifest::icons`). Ship at least a
  192 and a 512.
]

== Languages

One manifest per language, beside that language's #link("feeds.typ")[feeds] and
#link("search.typ")[search index]: `/manifest.webmanifest`,
`/fr/manifest.webmanifest`. Each is named in its own language (`site` under
`languages { fr { .. } }`), declares its own `lang` and `dir`, and starts and
scopes at its own root, so installing from `/fr/` gives a French app that stays
in the French site. Every page links the manifest of the language it is written
in.

== What it does not do

The manifest makes a site installable. It does not make it work offline: that
needs a service worker, which baudelaire does not generate. Write one into
`static/` and register it from your own script if you want one.
