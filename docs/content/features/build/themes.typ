#let frontmatter = (
  order: 15,
  title: "Themes",
  tags: ("feature", "themes", "templates"),
)
#import "/templates/theme.typ": callout

A theme ships a site's templates, assets, and config defaults as one unit. Name
one and your site gets a look without writing a template; override any part of
it by having your own file at the same path.

```kdl
theme "@preview/plume:1.0.0"
```

That is a Typst package spec, resolved through the same package store the
compiler already uses: downloaded once, cached across projects, versioned by the
registry. There is no second package manager and nothing to vendor.

A build that cannot reach `packages.typst.org` can name a mirror of it, which
covers the theme and every `@preview` package your pages import:

```kdl
typst { registry "https://packages.example.net" }
```

Only the `preview` namespace is ever downloaded. Any other namespace is read
from the local package directories (`~/.local/share/typst/packages/<namespace>/`
and the platform equivalents), which is where an unpublished theme can live
under a name of your own: install it as `@internal/plume:1.0.0` and every
project on the machine can name it.

== What a theme contains

A theme is laid out like a site, with fixed directory names (your `paths` name
*your* directories, and a theme cannot know what you called them):

```text
templates/   layouts a page can be bound to
assets/      stylesheets, scripts, images
static/      files copied verbatim
theme.kdl    config defaults
```

== Everything is a default

The rule is the same for all four, and it is the only rule: *your file wins*.

/ Templates: a page bound to `page.typ` uses `templates/page.typ` if you have
  one, and the theme's otherwise. Copy the theme's file into your own
  `templates/` and edit it; nothing else changes.
/ Assets: the theme's `assets/` are processed alongside yours, file by file. A
  theme shipping `assets/theme.css` and `assets/reset.css` while you ship your
  own `reset.css` gives you the theme's stylesheet and your reset.
/ Static: the same, for files copied verbatim.
/ Config: `theme.kdl` is a floor, not a ceiling. Every key you state wins; every
  key you leave out falls back to the theme's, nested blocks included.

#callout(kind: "note")[
  Because a theme's config is only a default, you can adopt one and keep your
  `site`, `url`, `content { collections }`, and everything else you already had.
  A theme that sets `html { pretty #false }` changes your build; a theme that
  sets `site` does not, because you set it too.
]

== Three to start from

The repository ships three, in `themes/`, each a complete look with templates,
assets, and config defaults:

/ #link("/themes/albatros/")[#raw("albatros")]: a centred blog. System sans at a
  comfortable measure, light and dark with a toggle, tag chips, reading time.
/ #link("/themes/spleen/")[#raw("spleen")]: a terminal. Monospace throughout, a
  prompt for a masthead, posts listed like a directory, dark first, and no
  JavaScript at all.
/ #link("/themes/voyage/")[#raw("voyage")]: a multilingual journal. Serif
  headings, a language switcher built from each page's own editions, every label
  read from the site's string table.

Each name links a live demo. The three are the same site built three times, so
whatever differs between them is the theme and nothing else.

Copy one into your project and name it, or install it into your package
directory and name it as `@local/albatros:0.1.0`. None of them hardcode a menu:
the nav is derived from #link("typst-modules.typ")[`@baudelaire/sections`], so it
follows `content/` instead of a list in config.

== Writing one

Point `theme` at a directory instead of a package and the same layering applies,
so a theme can be developed in place:

```kdl
theme "themes/plume"
```

The directory has to sit inside the project: a Typst import cannot reach outside
the project root, so a theme kept elsewhere would resolve its assets and then
fail on its first template. Publish it as a package when it is ready, and change
one line.

== What a template receives

Nothing about a theme changes the template contract, so a theme's `page.typ` is
an ordinary layout:

```typ
#let page(data, body) = {
  html.elem("main", body)
}
```

See #link("../../guide/config.typ")[configuration] for the data a layout is
handed, and #link("navigating.typ")[SPA and single-file export] for the output
modes a theme can turn on through its own `theme.kdl`.
