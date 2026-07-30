#let frontmatter = (
  order: 9,
  title: "Feeds & sitemap",
  tags: ("feature", "seo"),
)
#import "/templates/theme.typ": callout

Baudelaire emits a `sitemap.xml` of every page, plus RSS, Atom, and/or
#link("https://jsonfeed.org")[JSON Feed] feeds of your most recent dated pages.
Both are opt-in, and both need a canonical `url`.

```kdl
url "https://example.com"

generate {
  sitemap #true
  feed {
    formats "rss" "atom" "json"
    limit 20
  }
}
```

Each format writes its conventional file: `rss.xml`, `atom.xml`, `feed.json`.
A page joins the feed when its frontmatter has a `date`; `limit` caps how many
of the newest appear. The footer of this site links its
#link("/rss.xml")[RSS], #link("/atom.xml")[Atom], and
#link("/sitemap.xml")[sitemap].

Every page also advertises the feeds in its `<head>`, one
`link rel="alternate"` per configured format, which is how a reader or a browser
extension finds them without being told a URL.

== What an item carries

An entry takes its `title`, its `date`, its `description` (or `summary`), and
its taxonomy terms as categories from the page's frontmatter:

```typ
#let frontmatter = (
  title: "A tour of the cache",
  date: datetime(year: 2026, month: 1, day: 2),
  updated: datetime(year: 2026, month: 3, day: 4),
  description: "How a page decides it is still valid.",
  tags: ("build", "performance"),
)
```

Without a `description` a reader shows the title and nothing else, so it is
worth writing one: the same value fills the page's `<meta name="description">`
and its social preview.

The two dates are kept apart where a format keeps them apart. Atom emits
`published` and `updated`, JSON Feed `date_published` and `date_modified`; RSS
has only `pubDate`, which is the publication date.

#callout(kind: "note")[
  Feeds and the sitemap need absolute links, so enabling either without a `url`
  fails the build. On a preview host, pass `--base-url` to point them at the
  right origin.
]

== A feed per tag

```kdl
generate {
  feed {
    formats "rss"
    terms #true
  }
}
```

Adds a feed beside every taxonomy term's listing page, so a reader can follow
one tag instead of the whole site: `/tags/rust/rss.xml` next to `/tags/rust/`.
Each carries only that term's dated pages, in the same order and under the same
`limit` as the site feed, and identifies itself by its own URL so an aggregator
never merges it with another.

Term feeds follow the term pages, so a taxonomy needs `listing=#true` to have
them. They are off by default: one more file per term per format multiplies the
output of a heavily tagged site.
