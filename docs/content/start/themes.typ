#let frontmatter = (
  title: "Themes",
  order: 3,
)
#import "/templates/theme.typ": callout

A theme is a site's look packaged as one directory: templates, assets, and
config defaults. Name one and your pages have a design.

```kdl
theme "themes/albatros"
```

== Pick one

Four ship with baudelaire, one per kind of site. Pick by what the site *is*. The
look is the part you can change in an afternoon.

#table(
  columns: 3,
  align: (left, left, left),
  table.header([Theme], [For], [Shape]),
  [#link("/themes/albatros/")[albatros]], [A blog],
  [A centered column, tags, reading time, light and dark, a language switcher on
    a site with editions.],

  [#link("/themes/spleen/")[spleen]], [A blog, no JavaScript],
  [A terminal. Monospace throughout, a prompt for a masthead, posts as a
    directory listing, dark first. Not "minimal JS": none.],

  [#link("/themes/phares/")[phares]], [Documentation],
  [A sidebar built from your own `content/` tree, a search palette, the page's
    headings down the right, prev/next through the manual.],

  [#link("/themes/paysage/")[paysage]], [A portfolio],
  [A landing page, a work grid that builds itself from what you publish, one
    case study per project.],
)

Each name links a live demo. The #link("/themes/")[themes page] has all four.

== Adopt it

Copy the theme's directory into your project, then name it:

```kdl
theme "themes/albatros"
```

It has to sit inside the project root, because a Typst import can't reach
outside it.

Installed into your Typst package directory instead, a theme is named as a
package and shared across every project on the machine:

```sh
cp -r themes/albatros ~/.local/share/typst/packages/local/albatros/0.1.0
```

```kdl
theme "@local/albatros:0.1.0"
```

Only the `preview` namespace is ever downloaded. `@local` and any other
namespace are read from disk and never hit the network.

Scaffolding a new site straight onto a theme skips the `templates/` and
`assets/` it would shadow:

```sh
baudelaire init my-site --theme "themes/albatros"
```

Then read the theme's README for what it wants from a page. All four want a
`title`; most use `date`, `summary`, and a taxonomy.

#callout(kind: "note")[
  A theme's `theme.kdl` is a *floor*. Every key you state wins, nested blocks
  included, so adopting one never touches your `site`, `url`, or `author`.
]

#callout(kind: "warn")[
  Config lists replace wholesale. Declaring `content { collections { .. } }` of
  your own drops the theme's set rather than adding to it. Copy the theme's
  block first if you only meant to add a collection.
]

== Change part of it

One rule: *your file wins*, file by file, across `templates/`, `assets/` and
`static/`. Everything else still comes from the theme.

#table(
  columns: 2,
  align: (left, left),
  table.header([To change], [Do]),
  [A layout], [Copy `templates/page.typ` out of the theme into your own
    `templates/` and edit it.],
  [The look], [Ship your own `assets/style.css`. It replaces the theme's.],
  [A color or a width], [Copy the theme's `style.css` and edit the custom
    properties at the top. Every theme here declares its whole palette there.],
)

```css
/* assets/style.css, copied from the theme */
:root {
  --measure: 48rem;
  --accent: #7a3ea1;
}
```

Nothing else has to move. A theme you've overridden two files of is still a
theme you upgrade by replacing its directory.

Writing one of your own is #link("../write/theme-authoring.typ")[a page of its
own].
