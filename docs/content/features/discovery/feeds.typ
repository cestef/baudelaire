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

Term feeds follow the term pages, so a taxonomy needs `index=#true` to have
them. They are off by default: one more file per term per format multiplies the
output of a heavily tagged site.
