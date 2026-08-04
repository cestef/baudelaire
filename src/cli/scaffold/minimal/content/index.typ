#let frontmatter = (
  title: "Home",
)

This is the whole site: one config file, one page, one template. Nothing here is
load-bearing, so delete what you do not want.

A page is a Typst document with a `frontmatter` dictionary. Baudelaire reads that
dictionary, compiles the rest to HTML, and hands both to the layout `config.kdl`
names in `content { template }`; a page overrides it with a `template` key of its
own. `title` is the only key this page uses; `date`, `slug`, `tags`, `draft` and
`permalink` are understood when you need them, and any key Baudelaire does not
claim is passed through to your template untouched.

Add a directory under `content/` and its pages are served at the matching URLs.
Declare it as a collection in `config.kdl` when you want them ordered, given a
URL shape, or listed on an index page of their own.
