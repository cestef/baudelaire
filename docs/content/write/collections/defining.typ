#let frontmatter = (
  title: "Defining a collection",
  order: 8,
)
#import "/templates/theme.typ": callout

A collection is a group of pages sharing a sort order, a permalink shape and a
default template.

```kdl
content {
  collections {
    posts "posts/**/*.typ" {
      sort "date"
      reverse #true
      permalink "/posts/{slug}/"
      template "post.typ"
    }
  }
}
```

The node name is the collection id. The leading string is a glob selecting its
members, and the block tunes the rest.

You don't have to declare anything: a file in a subdirectory of `content/` joins
a collection named after that top directory, with every default below. Declaring
`posts { }` only changes what you name.

== Keys

#table(
  columns: 4,
  align: (left, left, left, left),
  table.header([Key], [Type], [Default], [Does]),
  [`glob`], [str], [the directory of the same name], [Which content files belong to the collection.],
  [`sort`], [`order` | `date` | `title`], [`order`], [What its members are ordered by.],
  [`reverse`], [bool], [`#false`], [Reverse that order.],
  [`permalink`], [template], [`/{path}/{slug}/`], [The URL pattern its pages publish at.],
  [`template`], [str], [--], [The layout its pages render through.],
  [`paginate`], [block], [--], [Generate an index over the collection.],
  [`feed`], [bool], [`#false`], [Also write a feed of its members, beside that index.],
  [`schema`], [block], [--], [What every member's frontmatter must declare.],
)

`paginate` is on #link("pagination.typ")[listings and pagination]; `feed` is on
#link("../../build/generate/feeds.typ")[feeds]; `schema` is on
#link("../frontmatter.typ")[frontmatter].

== Which files belong

The glob is matched against paths relative to `content/`, so it can reach
anywhere:

```kdl
content {
  collections {
    posts { glob "blog/**/*.typ" }
    notes "til/*.typ"
  }
}
```

Glob-configured collections claim their files first; whatever is left falls back
to the convention. A file directly under `content/` that no glob claims joins
`_root`: it publishes at `/{slug}/`, or at `/` if it is the `index`.

`_root` is a collection like any other, so configuring it is how the home page
and its neighbours get a layout without each naming one:

```kdl
content {
  collections {
    _root { template "page.typ" }
  }
}
```

A #link("../../start/themes.typ")[theme] states that in its own `theme.kdl`,
which is what lets a themed page render with nothing bound to it by hand.

== Order

`sort` picks the key, `reverse` flips it. Pages that share a key, or lack one
entirely, fall back to source path, so the order is stable across machines.

```kdl
content {
  collections {
    guide { sort "order" }
    blog  { sort "date"; reverse #true }
  }
}
```

This is the order that drives the #link("navigation.typ")[prev/next pager], the
generated #link("pagination.typ")[index pages], and the section tree a sidebar
is built from. Change it in one place and all three move together.

== Permalinks

`permalink` is a template of literal text and `{placeholder}` tokens.

#table(
  columns: 2,
  align: (left, left),
  table.header([Token], [Fills with]),
  [`{slug}`], [The page's slug.],
  [`{path}`], [Where the page sits under `content/`, however deep. Several segments at once.],
  [`{collection}`], [The collection's id, and nothing else.],
  [`{year}`, `{month}`, `{day}`], [The page's `date`, zero-padded. Empty when it has none.],
  [`{order}`], [The page's `order`. Empty when it has none.],
)

```kdl
content {
  collections {
    posts { permalink "/{year}/{month}/{slug}/" }
  }
}
```

`{path}` and `{collection}` are the same string for the ordinary layout, since a
collection *is* a directory. They part company once you nest: with the default
template, `content/ship/deploy/s3.typ` publishes at `/ship/deploy/s3/`, while
`/{collection}/{slug}/` would flatten it to `/ship/s3/`.

#callout(kind: "note")[
  A placeholder with nothing to render drops its segment rather than leaving an
  empty one, so a dateless page under `/{year}/{slug}/` lands at `/hello/`
  rather than `//hello/`. An unknown token is a config error with a span, not a
  silent fallback.
]

== Templates

`template` names the layout every member renders through, relative to
`paths { templates }`. A page overrides it with `template` in its own
#link("../frontmatter.typ")[frontmatter], and a page whose collection names none
is written out unwrapped. What the layout receives is on
#link("../templates.typ")[templates].
