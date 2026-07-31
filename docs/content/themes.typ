#let frontmatter = (
  title: "Themes",
  template: "page.typ",
)
#import "/templates/theme.typ": callout, link-to

A theme ships a site's templates, assets, and config defaults as one unit. Name
one and your site has a look without your writing a template; override any part
of it by having your own file at the same path.

Four ship with baudelaire, one per kind of site. Pick by what your site *is*.

/ #link("/themes/albatros/")[albatros] --- *you write posts*: a centred column
  at a comfortable measure, tag chips, reading time, light and dark, and a
  language switcher on a site that has editions.
/ #link("/themes/spleen/")[spleen] --- *you write posts and want no script*: a
  terminal. Monospace throughout, a prompt for a masthead, posts listed like a
  directory, dark first. Not "minimal JavaScript": none.
/ #link("/themes/phares/")[phares] --- *you document something*: a sidebar built
  from your own `content/` tree, a search palette on #html.elem("kbd")[/], the
  page's headings down the right, and prev/next that runs the length of the
  manual.
/ #link("/themes/paysage/")[paysage] --- *you show work*: a landing page, a grid
  of projects that builds itself from what you publish, and one case study per
  project.

Each name links a live demo, built from content that theme was designed for: the
blog themes share one set of posts, `phares` gets a small manual, `paysage` gets
a few projects.

== Getting one running

Three steps, and the first two are one line each.

+ *Copy the theme into your project.* It has to sit inside the project root,
  because a Typst import cannot leave it. From a checkout of the repository,
  that is `cp -r themes/albatros /path/to/site/themes/albatros`.

+ *Name it in `config.kdl`.*

  ```kdl
  theme "themes/albatros"
  ```

+ *Write a page it recognises.* Every theme wants a `title`; the rest is in its
  own README. For `albatros`, a post is:

  ```typ
  #let frontmatter = (
    title: "Hello",
    date: datetime(year: 2026, month: 7, day: 31),
    tags: ("intro",),
    summary: "One line, shown under the entry in the index.",
  )

  Body text.
  ```

  Put it in `content/posts/hello.typ` and build. The theme's `theme.kdl` already
  declared the `posts` collection, its index, the `tags` taxonomy, and the feeds.

Installed into your Typst package directory instead of copied, a theme is named
the way any dependency is, and every project on the machine can use it:

```kdl
theme "@local/albatros:0.1.0"
```

#callout(kind: "note")[
  Everything a theme provides is a *default*: your file at the same relative
  path wins, and your config wins key by key. Adopting one never touches your
  `site`, `url`, or `author`.

  The one exception is lists. Declaring `content { collections { .. } }`
  yourself replaces the theme's set rather than adding to it, so copy its block
  if you only meant to add a collection.
]

== Changing one

Copy the file you want to change out of the theme and into your own tree at the
same relative path: `templates/page.typ` for a layout, `assets/style.css` for
the look. Nothing else changes.

For a recolour, every theme here declares its palette as custom properties at
the top of its stylesheet, so restating a few of them is smaller than forking
the file:

```css
:root {
  --accent: #7a3ea1;
  --measure: 48rem;
}
```

The #link("/guide/themes/")[themes chapter] covers adopting, overriding, and
writing your own end to end;
#link("/features/build/themes/")[the reference] covers how the layering works.
The four live in
#link("https://github.com/cestef/baudelaire/tree/main/themes")[`themes/`] in the
repository, each with a README.
