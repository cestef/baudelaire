#let frontmatter = (
  title: "Frontmatter",
  order: 2,
)
#import "/templates/theme.typ": callout

A page's metadata is a Typst binding, not a header in another language:

```typ
#let frontmatter = (
  title: "Hello",
  date: datetime(year: 2026, month: 7, day: 9),
  tags: ("intro",),
)
```

The build reads that one top-level `frontmatter` export from the evaluated
module. Because Typst evaluates it, the value is an ordinary dict: it can be
computed from `sys.inputs`, from a loaded data file, or from anything else in
scope.

#callout(kind: "warn")[
  Single-element arrays need the trailing comma. `("intro")` is a string;
  `("intro",)` is a list.
]

== Recognized keys

#table(
  columns: 3,
  align: (left, left, left),
  table.header([Key], [Type], [Does]),
  [`title`], [str], [The page title, used in `<title>`, listings, feeds and social tags.],
  [`date`], [datetime], [When it was published. A page joins the feeds and sorts in a `sort "date"` collection only when it has one.],
  [`updated`], [datetime], [When it last changed materially. Reported as the sitemap's `lastmod` and a feed entry's `updated`, while `date` stays put.],
  [`expiry`], [datetime], [The last day it is published. After that it is left out of the build entirely, and `prune` removes what an earlier build wrote.],
  [`draft`], [bool], [Skip the page unless `--drafts` is passed.],
  [`slug`], [str], [Override the URL slug, otherwise derived from the filename.],
  [`lang`], [str], [Override the page's language, beating the filename suffix and the site default.],
  [`translation`], [str], [The key pairing this page with its editions in other languages.],
  [`template`], [str], [The template that wraps this page, overriding the collection's default.],
  [`order`], [int], [The sort key for a `sort "order"` collection.],
  [`redirect`], [list], [One old path, or a list of them, forwarded to this page.],
)

Values are typed. A key with the wrong type stops the build with a diagnostic
naming the file and the field, never a silent drop. `lang` and `translation`
only matter once a `languages` block exists; see
#link("i18n.typ")[multiple languages]. `redirect` is covered on
#link("collections/redirects.typ")[redirects].

== Taxonomy keys

Any key declared under `content { taxonomies }` is recognized too, and collects
a list of terms:

```typ
#let frontmatter = (
  title: "On Typst",
  tags: ("typst", "tooling"),
)
```

With `taxonomies { tags }` configured, this page is grouped under both terms.
See #link("collections/taxonomies.typ")[taxonomies].

== Everything else

Unknown keys aren't errors. They pass through as *extra* frontmatter, yours to
read in a #link("templates.typ")[template] as `page.frontmatter.<key>` and in a
listing as `entry.extra.<key>`.

Five of them the build reads itself, for
#link("../build/generate/meta.typ")[meta and social tags]:

#table(
  columns: 2,
  align: (left, left),
  table.header([Key], [Does]),
  [`description`], [The one-line summary in `<meta>`, the feed entry, and the announced record.],
  [`summary`], [An alias for `description`, used when there is no `description`.],
  [`image`], [The social preview image.],
  [`alt`], [Its alt text.],
  [`author`], [The page's author, overriding the site default.],
)

```typ
#let frontmatter = (
  title: "A post",
  description: "A short summary for search and social cards.",
  image: "cover.png",
  alt: "The cover, described for a reader who cannot see it",
)
```

#callout(kind: "note")[
  A key that is a near-miss of a recognized one (`titel`, `tag`, `redirects`) is
  treated as a typo and reported with a suggestion, not passed through.
]

== Schemas

Free-form keys are easy to get wrong: a template reading a `hero` that a page
never set renders an empty hole and the build stays green. A collection can say
what its members must carry:

```kdl
content {
  collections {
    blog {
      schema {
        title "str"
        tags "list"
        hero "str" optional=#true
        author
      }
    }
  }
}
```

Declaring a field requires it. `optional=#true` lets it be absent. A field with
no type (`author` above) is required but unconstrained. A page that breaks the
schema fails the build, pointing at the line in its frontmatter:

```text
  × frontmatter `hero` must be a string, but is of type `integer`
   ╭─[content/blog/post.typ:3:9]
 2 │   title: "Hello",
 3 │   hero: 3,
   ·         ┬
   ·         ╰── not the declared type
   ╰────
  help: write it as `hero: ".."`, or change the `blog` collection's schema
```

The types are the Typst ones a frontmatter dict can hold, not a second type
system: `str`, `bool`, `int`, `float`, `date` (a `datetime(..)`), `list` (an
array of strings), and `any`.

A recognized key can appear in a schema, to require it. Its type is already
fixed by the build, so declaring a different one (`title "int"`) is a config
error rather than a rule no page could satisfy.

#callout(kind: "note")[
  Schemas are per collection. A `blog` schema says nothing about `notes`, and a
  collection with no `schema` block constrains nothing.
]
