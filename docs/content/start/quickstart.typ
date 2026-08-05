#let frontmatter = (
  title: "Quickstart",
  order: 2,
)
#import "/templates/theme.typ": callout

A site running in five minutes. Assumes you've
#link("install.typ")[installed the binary].

```sh
baudelaire init my-site
cd my-site
baudelaire serve
```

That's a live site on #link("http://localhost:1821")[localhost:1821], rebuilding
on every save.

== Scaffold

`init` asks what to start from, then for a title, an author, a base URL and
whether to set up version control. It writes a `config.kdl`, a `content/` tree
with a couple of example pages, the `templates/` that render them, and an
`assets/style.css`. Pass `-y` to take every default and skip the prompts.

The first question offers the four starter shapes and the four
#link("themes.typ")[themes] the binary carries. Naming either on the command
line (`-t`, `--theme`) answers it, and the prompt is skipped.

The default shape is a blog. Four ship:

#table(
  columns: 2,
  align: (left, left),
  table.header([Shape], [Gives you]),
  [`blog`], [Dated posts, tags, pagination and feeds. The default.],
  [`docs`], [Ordered sections, sidebar nav, client-side search.],
  [`book`], [Ordered chapters, also exported as one HTML file.],
  [`minimal`], [One page and one template, nothing else.],
)

`--with` switches on optional features while scaffolding, and takes any of
`spa`, `standalone`, `speculation`, `search`, `pdf`:

```sh
baudelaire init my-docs -t docs --with search,spa
```

`--theme` scaffolds against a theme instead of a shape: the config states the
site's identity and paths, and the theme brings the templates, the assets and the
collections.

```sh
baudelaire init my-site --theme "themes/albatros"
```

A theme the binary carries is written into that directory on the spot, so this
is a site that builds. A spec naming anything else is a directory `init` cannot
fill, and it closes by saying so.

See #link("themes.typ")[themes].

== Serve

```sh
baudelaire serve
```

Binds `127.0.0.1:1821`, opens a browser, watches `content/`, `templates/`,
`assets/`, `static/` and `config.kdl`, and pushes a reload over Server-Sent
Events on every rebuild. `--port` and `--bind` override the config, `--no-open`
skips the browser. More on it in
#link("../build/preview.typ")[the dev server page].

Leave it running for the rest of this page.

== Write a page

```sh
baudelaire new posts/first-post --no-draft
```

That writes `content/posts/first-post.typ`, with the title derived from the
filename and today's date filled in (because the `posts` collection sorts by
date). Without `--no-draft` the page is written as a draft, and drafts are
excluded from the build until you pass `--drafts` or set
`content { drafts }`.

It arrives with the standard fields filled in. Open it and write:

```typ
#let frontmatter = (
  title: "First Post",
  date: datetime(year: 2026, month: 8, day: 2),
  tags: ("intro",),
  description: "Shown under the entry in the index.",
)

This is *real Typst*, not markdown. You get functions, variables, imports,
math like $e^(i pi) + 1 = 0$, and the whole standard library.

#let half-life(name, years) = [#name decays with a half-life of #years years.]
#half-life("Carbon-14", 5730)
```

Save it and the browser reloads. The post is at `/posts/first-post/`, listed on
`/posts/`, and filed under `/tags/intro/`.

#callout(kind: "warn")[
  A single-element Typst array needs the trailing comma. `tags: ("intro")` is
  the string `"intro"`, not a list of one tag.
]

The file's location decides its collection and its URL. `content/posts/` is the
`posts` collection because `config.kdl` says so, and everything about how a page
is grouped, ordered and addressed follows from there. See
#link("../write/pages.typ")[pages] and
#link("../write/collections/defining.typ")[collections].

Every key frontmatter understands is on
#link("../write/frontmatter.typ")[frontmatter]. `description` above fills the
meta tag, the feed entry and the search index, and a listing reads it back as
`entry.description`. Keys baudelaire doesn't claim are yours, and reach a
template as `entry.extra.<key>`.

== Build

```sh
baudelaire build
```

The site lands in `public/`, ready for any static host. A rebuild only touches
what you edited; everything else comes from cache. Add `--profile prod` to apply
a #link("../configure/profiles.typ")[profile], `-o dist` to write elsewhere.

#callout(kind: "tip")[
  `baudelaire check` compiles every page and reports broken internal links
  without writing anything. That plus `--strict`, which fails the run on any
  warning, is what you want in CI.
]

Then #link("../ship/deploy.typ")[deploy it].

== Where to go next

#table(
  columns: 2,
  align: (left, left),
  table.header([You want], [Go to]),
  [A look, without writing a template],
  [#link("themes.typ")[Themes]],

  [To change how pages group and get their URLs],
  [#link("../write/collections/defining.typ")[Collections]],

  [To render pages your own way],
  [#link("../write/templates.typ")[Templates]],

  [Every key `config.kdl` accepts],
  [#link("../configure/reference.typ")[Config reference]],

  [Every command and flag],
  [#link("../lookup/cli.typ")[CLI]],

  [Search, feeds, social cards, PDFs],
  [#link("../build/generate/search.typ")[Search],
    #link("../build/generate/feeds.typ")[Feeds],
    #link("../build/generate/cards.typ")[Cards],
    #link("../build/generate/pdf.typ")[PDFs]],

  [To put it online],
  [#link("../ship/deploy.typ")[Deploying]],
)
