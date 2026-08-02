#let frontmatter = (
  title: "Feeds & sitemap",
  order: 8,
)
#import "/templates/theme.typ": callout

The files crawlers, readers and agents go looking for: syndication feeds, a
sitemap, `robots.txt`, `llms.txt`. All four live in `generate { }`. Feeds and
the sitemap need a canonical `url`; the other two are better with one.

```kdl
url "https://example.com"

generate {
  sitemap #true
  feed {
    formats "rss" "atom"
    limit 20
  }
}
```

== Feeds

Each format writes its conventional file: `rss.xml`, `atom.xml`, `feed.json`
(#link("https://jsonfeed.org")[JSON Feed] 1.1). A page joins the feed when its
frontmatter has a `date`, and `limit` caps how many of the newest appear. Feeds
are per language, so a French site gets `/fr/rss.xml` listing only its own
posts.

#table(
  columns: 4,
  align: (left, left, left, left),
  table.header([Key], [Type], [Default], [Does]),
  [`formats`], [`rss` | `atom` | `json`], [--], [Which feed files to write. Naming none turns feeds off.],
  [`limit`], [int], [`20`], [How many of the newest dated pages a feed carries.],
  [`terms`], [bool], [`#false`], [Also write a feed beside every taxonomy term listing.],
)

Every page advertises the feeds in its `<head>`, one
`<link rel="alternate">` per configured format. That is how a reader or a
browser extension finds them without being told a URL.

=== What an item carries

```typ
#let frontmatter = (
  title: "A tour of the cache",
  date: datetime(year: 2026, month: 1, day: 2),
  updated: datetime(year: 2026, month: 3, day: 4),
  description: "How a page decides it is still valid.",
  tags: ("build", "performance"),
)
```

The title, the dates, the `description` (falling back to `summary`), and every
#link("../../write/collections/taxonomies.typ")[taxonomy] term as a flat category list. Without
a description a reader shows the title and nothing else, so write one: the same
value fills the page's `<meta name="description">` and its
#link("meta.typ")[social preview].

The two dates stay apart where the format keeps them apart. Atom emits
`published` and `updated`, JSON Feed `date_published` and `date_modified`. RSS
has only `pubDate`, which is the publication date.

A feed with nothing dated in it writes no file at all, rather than a valid but
empty one.

=== A feed per tag

```kdl
generate {
  feed {
    formats "rss"
    terms #true
  }
}
```

Adds `/tags/rust/rss.xml` next to `/tags/rust/`, carrying only that term's dated
pages under the same `limit`. Each identifies itself by its own URL, so an
aggregator never merges it with another.

Term feeds sit beside term listings, so the taxonomy needs `listing=#true`. The
build warns if it doesn't have it. They're off by default: one more file per
term per format multiplies the output of a heavily tagged site.

== Sitemap

```kdl
generate {
  sitemap #true
}
```

Writes `sitemap.xml` at the root, one `<url>` per built page as an absolute URL,
with a `lastmod` when the page is dated. A
#link("../../write/i18n.typ")[translated] page also carries an `xhtml:link`
alternate per edition plus an `x-default` pointing at the default language's, so
crawlers pair the translations.

#callout(kind: "note")[
  Feeds and the sitemap need absolute links, so asking for either without a
  `url` fails the build. On a preview host, pass `--base-url` to point them at
  the right origin.
]

== robots.txt

```kdl
generate {
  robots {
    disallow "/drafts/"
  }
}
```

The block's presence turns it on. You get one `User-agent: *` group with your
`disallow` lines, or a bare `Disallow:` (nothing blocked) if you name none. When
`sitemap` and `url` are both set, a `Sitemap:` line is appended.

== llms.txt

```kdl
generate {
  llms {
    summary "A Typst-native static site generator."
  }
}
```

Writes an #link("https://llmstxt.org")[llms.txt]: the site title as an H1, your
`summary` as a blockquote, then one `##` section per collection listing its
pages as Markdown links. One file per language, beside that language's feeds.

Without a `url` it's still written, with relative links, and the build warns.

The not-found page is the one page left out of all of this, and of the
#link("search.typ")[search index]: it's what a host answers an unmatched URL
with, not a destination.
