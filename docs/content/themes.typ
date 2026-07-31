#let frontmatter = (
  title: "Themes",
  template: "page.typ",
)
#import "/templates/theme.typ": callout, link-to

A theme ships a site's templates, assets, and config defaults as one unit. Name
one and your site has a look without your writing a template; override any part
of it by having your own file at the same path.

Three ship with baudelaire, and each demo below is the *same site* built with a
different one. Whatever differs between them is the theme and nothing else.

/ #link("/themes/albatros/")[albatros]: a centred blog. System sans at a
  comfortable measure, light and dark with a toggle, tag chips, reading time.
/ #link("/themes/spleen/")[spleen]: a terminal. Monospace throughout, a prompt
  for a masthead, posts listed like a directory, dark first, and no JavaScript
  at all.
/ #link("/themes/voyage/")[voyage]: a multilingual journal. Serif headings, a
  language switcher built from each page's own editions, every label read from
  the site's own string table.

== Using one

A theme directory has to sit inside the project that uses it, because a Typst
import cannot leave the project root. Copy one in, then name it:

```kdl
theme "themes/albatros"
```

Installed into your package directory instead, it is named the way any Typst
dependency is, and every project on the machine can use it without a copy:

```kdl
theme "@local/albatros:0.1.0"
```

#callout(kind: "note")[
  Everything a theme provides is a *default*: your file at the same relative
  path wins, and your config wins key by key. Adopting one never touches your
  `site`, `url`, or `author`.
]

See #link("/features/build/themes/")[the themes reference] for what a theme
contains, how the layering works, and how to write your own. The three above
live in
#link("https://github.com/cestef/baudelaire/tree/main/themes")[`themes/`] in the
repository.
