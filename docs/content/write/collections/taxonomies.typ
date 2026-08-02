#let frontmatter = (
  title: "Taxonomies",
  order: 10,
)
#import "/templates/theme.typ": callout

A taxonomy groups pages by a shared frontmatter list: tags, categories, a
series. Declare one and the build generates the index pages for it.

```kdl
content {
  taxonomies {
    tags listing=#true template="list.typ"
  }
}
```

Any page with `tags: ("rust", "cli")` in its
#link("../frontmatter.typ")[frontmatter] is now grouped automatically, and
`listing=#true` writes:

#table(
  columns: 2,
  align: (left, left),
  table.header([URL], [Holds]),
  [`/tags/`], [Every term, each with its page count as the entry's `note`.],
  [`/tags/rust/`], [Every page tagged `rust`, ordered by title.],
)

Both are ordinary templated pages rendered through the `template` you bound, so
they inherit the site layout. They receive the same
#link("pagination.typ")[structured listing data] a collection index does.

== Keys

A taxonomy is one `key=value` line, not a block.

#table(
  columns: 4,
  align: (left, left, left, left),
  table.header([Key], [Type], [Default], [Does]),
  [`key`], [str], [the taxonomy's id], [The frontmatter field its terms are read from.],
  [`listing`], [bool], [`#false`], [Generate a page per term, and an index of the terms.],
  [`template`], [str], [--], [The layout those listings render through.],
  [`paginate`], [int], [--], [Members per term page. Without it, every member sits on one.],
  [`prefix`], [str], [`page`], [The path segment before a term page's number.],
)

== Reading a different key

A taxonomy reads the frontmatter key that matches its own name. Point it
elsewhere with `key=` to group the same content more than one way, or to name
the taxonomy independently of the field:

```kdl
content {
  taxonomies {
    tags   listing=#true template="list.typ"
    topics key="categories" listing=#true template="list.typ"
  }
}
```

== Paginating a term

A term listing holds every page under it, which on a blog with three years of
Rust posts is one page listing four hundred. `paginate=` chunks it by the same
rule a collection index is chunked by, so page 2 is named the same way in both:

```kdl
content {
  taxonomies {
    tags listing=#true paginate=20
  }
}
```

`/tags/rust/` keeps the first twenty, `/tags/rust/page/2/` takes the next, and
each carries `page.frontmatter.nav` like any other paginated listing. `prefix=`
renames the `page` segment, or empties it for `/tags/rust/2/`.

== Terms and slugs

Terms are the strings you write, unchanged, in listings and titles: a term page
is titled `Tags: rust`. The URL segment is the slugged form, lowercased with
runs of non-alphanumerics collapsed to `-`.

#callout(kind: "warn")[
  Two terms that slug to the same segment (`C++` and `C--` both give `c`) are a
  build error naming both, not a silent overwrite. A term with no letters or
  digits at all has no slug and errors the same way.
]

On a multi-language site terms are grouped per language, so a French and an
English `rust` tag are separate `/fr/tags/rust/` and `/tags/rust/` pages, never
a merged one. See #link("../i18n.typ")[multiple languages].

A term can also carry its own feed, with `generate { feed { terms #true } }`;
see #link("../../build/generate/feeds.typ")[feeds and sitemap].
