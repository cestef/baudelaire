#let frontmatter = (
  title: "Themes",
  template: "page.typ",
)
#import "/templates/theme.typ": callout

A theme is a site's templates, assets, and config defaults shipped as one unit.
Name one in `config.kdl` and you have a site without writing a template:

```kdl
theme "themes/albatros"
```

Four ship with baudelaire, one per kind of site. Each name links a live demo,
built from content that theme was designed for.

#table(
  columns: 3,
  align: (left, left, left),
  table.header([Theme], [For], [Looks like]),
  link("/themes/albatros/")[albatros],
  [You write posts.],
  [Centered column, tag chips, reading time, light and dark, language switcher.],

  link("/themes/spleen/")[spleen],
  [You write posts and want no script.],
  [A terminal. Monospace throughout, dark first, zero JavaScript.],

  link("/themes/phares/")[phares],
  [You document something.],
  [Sidebar from your own `content/` tree, search palette, headings down the right.],

  link("/themes/paysage/")[paysage],
  [You show work.],
  [Landing page, project grid that builds itself, one case study per project.],
)

== Run one

```sh
cp -r themes/albatros /path/to/site/themes/albatros
```

```kdl
theme "themes/albatros"
```

Then write a page it recognizes. Every theme wants a `title`, and each README
lists the rest. An `albatros` post:

```typ
#let frontmatter = (
  title: "Hello",
  date: datetime(year: 2026, month: 7, day: 31),
  tags: ("intro",),
  summary: "One line, shown under the entry in the index.",
)

Body text.
```

Save it as `content/posts/hello.typ` and build. The theme's `theme.kdl` already
declared the `posts` collection, its index, the `tags` taxonomy, and the feeds.

The directory has to sit inside your project root, since a Typst import can't
leave it. Installed into your Typst package directory instead, a theme is named
like any other dependency and every project on the machine can use it:

```kdl
theme "@local/albatros:0.1.0"
```

#callout(kind: "note")[
  Everything a theme provides is a default: your file at the same path wins, and
  your config wins key by key. Lists are the exception. Declaring
  `content { collections { .. } }` yourself replaces the theme's set instead of
  adding to it, so copy its block if you only meant to add one.
]

== Change one

Copy the file you want to change into your own tree at the same relative path:
`templates/page.typ` for a layout, `assets/style.css` for the look. Nothing else
moves.

For a recolor, every theme declares its palette as custom properties at the top
of its stylesheet, so restating a few beats forking the file:

```css
:root {
  --accent: #7a3ea1;
  --measure: 48rem;
}
```

#link("start/themes.typ")[Picking and adopting a theme] is the guide, and
#link("write/theme-authoring.typ")[writing your own] is the page after it.
The four live in
#link("https://github.com/cestef/baudelaire/tree/main/themes")[`themes/`], each
with a README.
