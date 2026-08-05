#let frontmatter = (
  title: "Pages",
  order: 1,
)
#import "/templates/theme.typ": callout

A page is a `.typ` or `.md` file under `content/`. Where it sits decides its URL,
and everything after the frontmatter binding is the body. This page describes
`.typ`; #link("markdown.typ")[Markdown pages] covers what differs.

```typ
#let frontmatter = (
  title: "Hello",
  date: datetime(year: 2026, month: 7, day: 9),
)

This is *real Typst*. Functions, math $x^2$, tables and data loading,
not just prose.
```

Save that as `content/blog/hello.typ` and it publishes at `/blog/hello/`.

`baudelaire new blog/hello` writes the file for you, with today's date and
`draft: true`. Pass `-b` to get `blog/hello/index.typ` instead, so images can
sit next to the page.

== Path to URL

#table(
  columns: 2,
  align: (left, left),
  table.header([Source file], [URL]),
  [`content/index.typ`], [`/`],
  [`content/about.typ`], [`/about/`],
  [`content/blog/hello.typ`], [`/blog/hello/`],
  [`content/blog/hello/index.typ`], [`/blog/hello/`],
  [`content/docs/deploy/s3.typ`], [`/docs/deploy/s3/`],
)

The default permalink is `/{path}/{slug}/`: the directories the page sits under,
however deep, then its slug. A #link("collections/defining.typ")[collection] can set a
different one.

A file named `index` publishes at its directory's own URL, which is what makes
the fourth row a *page bundle*: the page and its images in one folder.
`content { index }` renames that stem if you prefer `_index`.

=== Slugs

The slug comes from the file stem, lowercased, with runs of anything that isn't
a letter or digit collapsed to a single `-`. Letters are Unicode, so `café.typ`
stays `/café/` rather than losing its accent. Override it in frontmatter:

```typ
#let frontmatter = (
  title: "Hello, world",
  slug: "hello",
)
```

Two suffixes on the file stem are markers rather than slug text. `post.draft.typ`
is a draft, and `post.fr.typ` is the French edition (see
#link("i18n.typ")[multiple languages]). They stack in either order.

=== Clean or flat

```kdl
links {
  style "flat"
}
```

`clean` (the default) publishes `/blog/hello/`; `flat` publishes
`/blog/hello.html`. The site root is `/` either way, and the style applies to
the permalink as well as the file, so canonical tags, feeds and the sitemap all
agree with what's actually served.

== The body

It's Typst, so anything a Typst document can do a page can do:

```typ
== Headings, lists, tables

- markup
- $ sum_(k = 1)^n k = (n (n + 1)) / 2 $

#table(
  columns: 2,
  table.header([Format], [File]),
  [RSS], [`rss.xml`],
)

#let rows = csv("data/crates.csv")
#for row in rows.slice(1) [- #row.at(0) ]
```

Fenced code blocks are highlighted by the compiler, not by a script in the
browser (see #link("highlighting.typ")[code highlighting]). Images written with
`#image("cover.png")` resolve relative to the page and go through the
#link("../build/images.typ")[image pipeline].

#callout(kind: "tip")[
  A page is a program. `#for`, `#let` and `csv(..)` run at build time, so a
  table can be generated from a data file instead of typed out.
]

== Links between pages

Link the *source path*, not the URL. The build rewrites it to the target's
permalink:

```typ
See #link("../start/quickstart.typ")[the quickstart], or
#link("frontmatter.typ")[frontmatter].
```

Relative paths resolve against the linking file; a leading `/` resolves against
the project root, the same way a Typst import does. Fragments come along:
`#link("pages.typ#slugs")` lands on the heading.

Only source targets are rewritten, which means `.typ` and, where markdown is on,
`.md`. Every other href passes through as authored, so a link to `/robots.txt` or
an external URL is left alone.

#callout(kind: "warn")[
  A source link with no page behind it fails the build. `links { strict #false }`
  downgrades that to a warning, and `baudelaire check` reports broken links
  without writing any output.
]

Rename a page and the links to it keep working, because they name the file, not
the URL. Move it and they break loudly at build time.

== Whether a page builds at all

`draft: true` pages are skipped unless you pass `--drafts`. A `date` in the
future is skipped unless you pass `--future` or set `content { future #true }`.
An `expiry` date drops the page for good. All three are
#link("frontmatter.typ")[frontmatter keys].

== The not-found page

`content/404.typ` is the page a host serves for a URL that matches nothing. It
publishes as a flat `404.html` whatever `links { style }` says, because that is
the file a static host looks for, and it is the one page left out of listings,
feeds, the sitemap and the prev/next pager: it is what an unmatched URL lands on,
not a destination.

Every #link("../start/quickstart.typ")[starter shape] writes one. A site without
one builds fine and the visitor gets the host's generic page instead, so the
build says so once:

```text
☞ no not-found page: an unmatched URL gets the host's, not yours
  help: write `content/404.typ`, which publishes as `404.html`
```

With #link("i18n.typ")[languages] declared, `404.fr.typ` publishes at
`/fr/404.html`, and both the dev server and a host that maps directories answer a
French URL with it.
