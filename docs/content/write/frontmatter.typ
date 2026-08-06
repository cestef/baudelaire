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

A #link("markdown.typ")[markdown page] declares the same keys in a fenced block
instead, in YAML, TOML or KDL. Every key on this page behaves identically there:
the block is read into the same dict, and everything below is the same walk.

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
  [`date`], [datetime | str], [When it was published. A page joins the feeds and sorts in a `sort "date"` collection only when it has one.],
  [`updated`], [datetime | str], [When it last changed materially. Reported as the sitemap's `lastmod` and a feed entry's `updated`, while `date` stays put.],
  [`expiry`], [datetime | str], [The last day it is published. After that it is left out of the build entirely, and `prune` removes what an earlier build wrote.],
  [`draft`], [bool], [Skip the page unless `--drafts` is passed.],
  [`slug`], [str], [Override the URL slug, otherwise derived from the filename.],
  [`path`], [str], [Publish at exactly this URL, replacing the collection's permalink and the slug both.],
  [`lang`], [str], [Override the page's language, beating the filename suffix and the site default.],
  [`translation`], [str], [The key pairing this page with its editions in other languages.],
  [`template`], [str], [The template that wraps this page, overriding the collection's default.],
  [`order`], [int], [The sort key for a `sort "order"` collection.],
  [`redirect`], [list], [One old path, or a list of them, forwarded to this page.],
  [`source`], [str], [The name of a `paths { sources }` entry whose file is this page's body. Markdown pages only.],
)

Values are typed. A key with the wrong type stops the build with a diagnostic
naming the file and the field, never a silent drop. The three dates take either a
Typst `datetime` or a string spelled exactly `YYYY-MM-DD`, so a day needs no
`datetime(..)` call:

```typ
date: "2026-08-05",
```

`lang` and `translation`
only matter once a `languages` block exists; see
#link("i18n.typ")[multiple languages]. `redirect` is covered on
#link("collections/redirects.typ")[redirects].

== Naming a URL outright

`path` publishes a page where you say, whatever its collection's
#link("collections/defining.typ")[permalink] pattern would have produced:

```typ
#let frontmatter = (
  title: "About",
  path: "/about-us/",
)
```

Leading and trailing slashes are optional, so `about-us` is the same URL. A path
whose last segment carries an extension names a *file* and publishes as one,
which is what a site arriving from Jekyll needs:

#table(
  columns: 2,
  align: (left, left),
  table.header([`path`], [Writes]),
  [`/about-us/`], [`about-us/index.html`, served at `/about-us/`],
  [`/2019/post.html`], [`2019/post.html`, served at that URL],
)

It is the migration escape hatch: copy each old URL onto the page that answers
it and the URL set matches by construction, with no permalink archaeology. Two
pages claiming one path is an error naming both files, not a race.

`path` does not touch the page's identity. Translations still pair on
`collection/slug`, so each edition may state its own path and stay the same page
in another language; an edition that states none publishes under its language
prefix as usual.

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
system:

#table(
  columns: 2,
  align: (left, left),
  table.header([Type], [Holds]),
  [`str`, `bool`, `int`, `float`], [The scalar of that name.],
  [`date`], [A `datetime(..)`, with or without a time of day.],
  [`list<T>`], [An array whose every element is a `T`. Bare `list` is `list<str>`.],
  [`dict`], [A dictionary, whose own fields a block declares.],
  [`any`], [Anything: the field must merely be there.],
)

A list says what it holds, so the parameter nests as deep as the data does:

```kdl
schema {
  widths "list<int>"
  matrix "list<list<int>>"
  author "dict" {
    name "str"
    email "str" optional=#true
  }
  authors "list<dict>" {
    name "str"
  }
}
```

A block declares the fields of the dictionary the type ends in, through however
many lists wrap it. Nested fields are checked the same way, and are required
unless they say `optional=#true` themselves. The diagnostic names the one that
broke, down to the element:

```text
  × frontmatter `authors.1.name` must be a string, but is of type `integer`
```

A recognized key can appear in a schema, to require it. Its type is already
fixed by the build, so declaring a different one (`title "int"`) is a config
error rather than a rule no page could satisfy.

#callout(kind: "note")[
  Schemas are per collection. A `blog` schema says nothing about `notes`, and a
  collection with no `schema` block constrains nothing.
]
