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

output {
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
