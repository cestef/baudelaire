#let frontmatter = (
  title: "Frontmatter",
  order: 3,
)
#import "/templates/theme.typ": callout

Every page carries a little metadata: its title, its date & the template that
wraps it. In Baudelaire that metadata is a Typst binding, not a separate header
in another language.

== The binding

Frontmatter is a top-level `#let frontmatter = (…)` export. The build reads that
one binding from the page's evaluated module; everything after it is the body.

```typ
#let frontmatter = (
  title: "Hello",
  date: datetime(year: 2026, month: 7, day: 9),
  tags: ("intro",),
)
```

Because Typst evaluates it, the value is a dict like any other, so it can be computed, e.g. a title derived from a filename, a date from `sys.inputs`, or fields pulled from a loaded data file.

#callout(kind: "warn")[
  The older `#frontmatter(…)` *call* form was removed. A page that still uses it
  fails with a migration error pointing at the binding: export a dict with
  `#let frontmatter = (title: "…")`
]

== Recognized keys

/ #raw("title:str"): The page title (a string). Used in the `<title>`, headings,
  navigation, feeds, and social tags.
/ #raw("date:datetime"): A Typst `datetime`. A page joins its
  #link("../features/feeds.typ")[feeds] and sorts in a `sort="date"` collection
  only when it has one. Write it as `datetime(year: 2026, month: 7, day: 9)`.
/ #raw("draft:bool"): Draft pages are skipped unless you pass `--drafts`
  (or a profile turns them on).
/ #raw("slug:str"): Override the URL slug, otherwise derived from the filename.
/ #raw("template:str"): The template that wraps this page, overriding the
  collection's default.
/ #raw("order:int"): An integer sort key for a `sort="order"` collection (the guide
  chapters here use it).
/ #raw("redirect:array<str>"): One old path, or a list of them, to forward to this page.
  See #link("../features/redirects.typ")[redirects].

Values are typed: a key with the wrong type is a hard error naming the file and
field, never a silent drop. `title: 3` or `tags: "intro"` (a bare string where a
list is expected) stops the build with an explanation.

== Taxonomy keys

Any key you declare under `taxonomies` in `config.kdl` is recognized too, and
collects a list of terms:

```typ
#let frontmatter = (
  title: "On Typst",
  tags: ("typst", "tooling"),
)
```

With `taxonomies { tags }` configured, `tags` groups the page under each term.
See #link("../features/taxonomies.typ")[taxonomies].

== Everything else

Unknown keys are not errors — they pass through as *extra* frontmatter, yours to
read in a template. `description`, `summary`, `image`, and `author` are common
ones the build itself reads when present (for
#link("../features/meta.typ")[meta and social tags]):

```typ
#let frontmatter = (
  title: "A post",
  description: "A short summary for search and social cards.",
  image: "cover.png",
)
```

A template reaches them through the page's `frontmatter` dict like any other
field.

#callout(kind: "note")[
  A key that is a near-miss of a recognized one — `titel`, `tag`, `redirects` —
  is treated as a typo and reported with a "did you mean?" suggestion rather than
  silently passed through.
]
