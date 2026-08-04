#let frontmatter = (
  title: "Home",
)

Welcome to your new Baudelaire site. Every page here is Typst, compiled to HTML
and wrapped by the templates your theme ships. Edit `content/index.typ` to change
this page, and drop new pages beside it or under `content/posts/`.

= What the theme decides

The layout each page renders through, the collections it groups pages into, and
what a listing looks like all come from the theme's `theme.kdl`. Your own
`config.kdl` overrides any of it key by key, and a file you write in
`templates/` or `assets/` shadows the theme's file of the same name.

Read the theme's README for the frontmatter it reads: all of them use `title`,
and most use `date`, `summary`, and a taxonomy.

= What is yours

#link("https://baudelaire.cstef.dev/write/frontmatter/")[Frontmatter] keys
Baudelaire does not claim are passed through untouched, so a template can read
anything you put there.
