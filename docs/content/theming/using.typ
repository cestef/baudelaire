#let frontmatter = (
  title: "Themes",
  order: 1,
)
#import "/templates/theme.typ": callout

A theme is a site's look packaged as one directory: templates, assets, and
config defaults. Name one and your pages have a design.

```kdl
theme "themes/albatros"
```

== Pick one

Four ship with baudelaire, one per kind of site. Pick by what the site is. You
can change the look in an afternoon.

#table(
  columns: 3,
  align: (left, left, left),
  table.header([Theme], [For], [Shape]),
  [#link("/themes/albatros/")[albatros]], [A blog],
  [A centered column. Bylines with avatars, lead images, tags, reading time, an
    opt-in contents list, related posts, light and dark, a language switcher on
    a site with editions.],

  [#link("/themes/spleen/")[spleen]], [A blog, no JavaScript],
  [A terminal. Monospace throughout, a prompt for a masthead, posts as a
    directory listing, dark first. No JavaScript at all.],

  [#link("/themes/phares/")[phares]], [Documentation],
  [A sidebar built from your own `content/` tree, a search palette, the page's
    headings down the right, breadcrumbs, an edit link, prev/next through the
    manual, and the components a manual writes with: callouts, tabs, steps,
    cards, badges.],

  [#link("/themes/paysage/")[paysage]], [A portfolio],
  [A landing page, a work grid that builds itself from what you publish, one
    case study per project.],
)

Each name links a live demo.

All four ship a `not-found.typ`. Bind it from `content/404.typ`, which publishes
as the flat `404.html` a static host serves for an unmatched URL:

```typ
#let frontmatter = (title: "Not found", template: "not-found.typ")
```

== Get one

All four are in the binary. Write one into the project:

```sh
baudelaire theme list          # the four, one line each
baudelaire theme add albatros  # writes themes/albatros/
```

Nothing is downloaded. The files land in your project, and you can read, edit
or replace them.

A theme from anywhere else uses the same command with a different spec:

```sh
baudelaire theme add ./elsewhere/plume            # a directory on this machine
baudelaire theme add @preview/plume:1.0.0         # the Typst package store
baudelaire theme add gh:owner/plume#v1.2.0        # a forge repository, at a tag
baudelaire theme add https://x.dev/plume.tar.gz   # an archive
```

A repository or an archive that holds a whole project takes `--subdir
themes/plume`, naming the directory the theme is in. Whichever it came from, the
copy records where it came from, so `update` goes back there. The
#link("../lookup/cli.typ")[CLI page] has the whole table.

`add` leaves a small record beside them saying which files it wrote, which is
what later lets `baudelaire theme update <name>` bring the untouched ones
forward without walking over your edits:

```sh
baudelaire theme info albatros    # what it ships, and what your copy has become
baudelaire theme update albatros  # keeps every file you have changed
baudelaire theme remove albatros  # and this keeps them too, unless --force
```

The #link("../lookup/cli.typ")[CLI page] has the whole table of what each verb
does to a file you have edited, deleted, or added.

== Adopt it

Name the directory it landed in:

```kdl
theme "themes/albatros"
```

It has to sit inside the project root, because a Typst import can't reach
outside it. `--dir` puts it somewhere else inside the root; name that path
instead.

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

Scaffolding a new site straight onto a theme writes only what a theme cannot
decide: the site's identity, its paths, and a preview. It writes no templates,
no assets and no `collections`, since a `collections` list of yours would
replace the theme's.

```sh
baudelaire init my-site --theme "themes/albatros"
```

A spec naming one of the four is written on the spot, so that one command is a
project that builds:

```text
◆ theme
➜ albatros  11 files to themes/albatros
```

A spec naming anything else is yours to put there, and the run tells you so.

`--template` is not used with `--theme`: what a starter shape would contribute
is the part the theme already declares.

Then read the theme's README for what it wants from a page. All four want a
`title`; most use `date`, `summary`, and a taxonomy.

#callout(kind: "note")[
  A theme's `theme.kdl` is a floor of defaults. Every key you state wins,
  nested blocks included, so adopting one never touches your `site`, `url`, or
  `author`.
]

#callout(kind: "warn")[
  Config lists replace wholesale. Declaring `content { collections { .. } }` of
  your own drops the theme's set rather than adding to it. Copy the theme's
  block first if you only meant to add a collection.
]

== Change part of it

Your file wins, file by file, across `templates/`, `assets/` and `static/`.
Everything else still comes from the theme.

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

A theme with two overridden files still updates: `baudelaire theme update`
keeps the two files you changed and rewrites the rest.

To write a theme of your own, see #link("authoring.typ")[writing a theme].
