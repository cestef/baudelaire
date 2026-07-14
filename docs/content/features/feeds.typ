#let frontmatter = (
  order: 8,
  title: "Feeds & sitemap",
  tags: ("feature", "seo"),
)
#import "/templates/theme.typ": callout

With a canonical `url` set, Baudelaire emits a `sitemap.xml` of every page plus
RSS, Atom, and/or #link("https://jsonfeed.org")[JSON Feed] feeds of your most
recent dated pages.

```kdl
url "https://example.com"

output {
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
  Feeds and the sitemap need absolute links, so they only emit when `url` is
  set. On a preview host, pass `--base-url` to point them at the right origin.
]
