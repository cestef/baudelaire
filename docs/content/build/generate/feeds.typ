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
  [`content`], [`summary` | `full`], [`summary`], [How much of each page an entry carries. See #link(<content>)[Full entries].],
  [`terms`], [bool], [`#false`], [Also write a feed beside every taxonomy term listing.],
  [`names`], [block], [--], [What each format's file is called, one key per format.],
)

=== Full entries <content>

By default an entry carries the page's one-line `description` and nothing else,
so a reader that renders entries in place has nothing to render and every
subscriber has to open the site. `content "full"` sends the prose too:

```kdl
generate {
  feed {
    formats "rss" "atom"
    content "full"
  }
}
```

The body is taken from `html { region }`, the same part of the page the
#link("search.typ")[search index] reads, so the header, sidebar and footer
around it never travel with it, and neither does anything `region { ignore }`
names *inside* it. Scripts and stylesheets are dropped too: a reader runs
neither, and both would arrive as text.

A layout that emits no region at all falls back to `<body>`, never to the whole
document. That is where a feed parts company with the search index, which counts
a page with no region whole rather than not at all. Name a region if your pages
carry chrome, or every entry carries it too.

The summary is not replaced. Each format has a place for both, and a reader's
list view wants the short one:

#table(
  columns: 3,
  align: (left, left, left),
  table.header([Format], [Summary], [Body]),
  [RSS], [`<description>`], [`<content:encoded>`],
  [Atom], [`<summary>`], [`<content type="html">`],
  [JSON Feed], [`summary`], [`content_html`],
)

#callout(kind: "warn")[
  A full feed publishes the whole post. That is the point for a personal blog and
  is not always what a site wants: an aggregator that reprints it is reprinting
  all of it, and readers may never arrive.
]

=== Keeping an old feed URL

A feed is the one artifact a redirect cannot save: a reader fetches the file and
never renders the stub's meta refresh. Coming from a generator that named it
differently, keep the name:

```kdl
generate {
  feed {
    formats "rss"
    names { rss "index.xml" }   // Hugo's name; Jekyll's is feed.xml
  }
}
```

The build writes that file, the feed's own `<id>` claims it, and every page's
autodiscovery tag points at it. A format with no override keeps the conventional
name.

Every page advertises the feeds in its `<head>`, one
`<link rel="alternate">` per configured format. That is how a reader or a
browser extension finds them without being told a URL.

=== What the channel says

A feed states what the whole site is, next to its title. That is the top-level
`description`:

```kdl
site "Fernweh"
description "Notes from the road."
```

It fills RSS's mandatory `<description>`, Atom's optional `<subtitle>`, and JSON
Feed's `description`, and a language states its own beside its `site` (see
#link("../../write/i18n.typ")[multiple languages]). Unset, RSS repeats the feed's
title, because the element cannot be omitted.

#callout(kind: "note")[
  It is not a fallback for a page's own `<meta name="description">`. One
  sentence stamped on every page is duplicate metadata, which is worse than a
  page having none. `generate { llms { summary } }` does fall back to it: both
  answer the same question about the site.
]

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

=== A feed per collection

The site feed carries everything dated, which is the wrong granularity for a
site that publishes more than one kind of thing: a reader who wants the essays
takes the release notes too. Ask on the collection:

```kdl
content {
  collections {
    posts { feed #true; paginate { template "list.typ" } }
  }
}
```

Adds `/posts/rss.xml` next to `/posts/`, carrying only that collection's dated
pages under the same `limit`. Members of the collection advertise it in their
`<head>` alongside the site feed, named `Site - Posts` so a reader tells the two
apart in a subscribe dialog.

Per collection rather than one flag over all of them, because most collections
want no feed: nobody subscribes to a `docs` tree.

#table(
  columns: 2,
  align: (left, left),
  table.header([Ask for], [Get]),
  [`feed` on a collection], [that collection's members, at its index],
  [`feed { terms }`], [one per taxonomy term, at each term listing],
  [neither], [the site feed alone, carrying everything dated],
)

#callout(kind: "warn")[
  A feed's home is the index it sits beside, so the collection needs a
  `paginate { }` block; the build says so if it has none. A collection mounted
  at `/` wants the site feed's own file and is told the same way: the site feed
  keeps it.
]

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
