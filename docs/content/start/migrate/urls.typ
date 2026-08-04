#let frontmatter = (
  title: "Keeping your URLs",
  order: 9,
)
#import "/templates/theme.typ": callout

A migration that changes paths loses inbound links, search rankings and every
bookmark. Most old shapes are one `permalink` line; the rest are redirects.

== Say it outright

The blunt instrument, and the one that cannot be got wrong: a page names the URL
it answers at.

```typ
#let frontmatter = (
  title: "The night train to Vienna",
  path: "/2019/03/night-train.html",
)
```

Whatever the old site's permalink rules were, a converter can copy each old URL
onto the page that answers it and the URL set matches by construction. A path
ending in a file name publishes as that file, which is the shape a Jekyll site
carries. See #link("../../write/frontmatter.typ")[frontmatter].

The patterns below are what you want when the old URLs follow a *rule*: one line
per collection beats one line per page, and new pages then land in the same shape
without being told.

== The default

`/{path}/{slug}/`: the directories a page sits under, then its slug. A collection
sets its own with #link("../../write/collections/defining.typ")[`permalink`],
from these tokens:

#table(
  columns: 2,
  align: (left, left),
  table.header([Token], [Fills with]),
  [`{slug}`], [The page's slug.],
  [`{path}`], [Its directories under `content/`, however deep.],
  [`{collection}`], [The collection id, one segment.],
  [`{year}`, `{month}`, `{day}`], [Its `date`, zero-padded.],
  [`{order}`], [Its `order`.],
)

== Common shapes

#table(
  columns: 3,
  align: (left, left, left),
  table.header([Old URL], [Came from], [Say]),
  [`/blog/hello/`], [Zola, Hugo, the default], [nothing],
  [`/2026/07/hello/`], [a dated archive], [`permalink "/{year}/{month}/{slug}/"`],
  [`/2026/07/09/hello.html`], [Jekyll], [`permalink "/{year}/{month}/{day}/{slug}"` and `links { style "flat" }`],
  [`/hello/`], [a flat blog], [`permalink "/{slug}/"`],
  [`/blog/hello.html`], [an older site], [`links { style "flat" }`],
)

`links { style }` is site-wide: it decides whether every page is a directory with
an `index.html` or a bare `.html` file, canonical tags, feeds and sitemap
included.

#callout(kind: "warn")[
  Jekyll takes the date out of the filename, baudelaire does not.
  `2026-07-09-hello.md` becomes the slug `2026-07-09-hello` unless you rename the
  file or set `slug: "hello"` in its frontmatter. Do it before the permalink, or
  the token you added lands next to a date you already had.
]

== Listings

#table(
  columns: 2,
  align: (left, left),
  table.header([URL], [Comes from]),
  [`/blog/`], [`paginate { }` on the `blog` collection.],
  [`/blog/page/2/`], [`paginate { size 5 }`. `prefix` renames `page`, `mount` moves page 1.],
  [`/tags/`, `/tags/rust/`], [`taxonomies { tags listing=#true }`.],
)

A taxonomy publishes under its own id, so the id is the URL segment. To serve
Hugo's `categories` at `/topics/`, name the taxonomy `topics` and point it at the
old frontmatter key:

```kdl
content {
  taxonomies {
    topics key="categories" listing=#true template="list.typ"
  }
}
```

== Everything that still moved

List the old paths on the page that replaced them:

```typ
#let frontmatter = (
  title: "Configuration",
  redirect: ("/old/config/", "/setup/"),
)
```

That writes an HTML stub at each old path: a meta refresh, a canonical link, a
manual anchor. It works on any host. On Netlify or Cloudflare Pages, turn the
same declarations into real 301s:

```kdl
generate {
  redirects #true
}
```

Both are on #link("../../write/collections/redirects.typ")[redirects], including
why the two cannot be served at once.

#callout(kind: "warn")[
  `redirect` is a frontmatter key, so only an authored page can claim an old
  path. A generated listing (a collection index, a term page) has no source file
  and can claim nothing, which is why Zola's `/blog/page/1/` and Hugo's
  `/posts/page/1/` have no equivalent here: those are aliases of a page that is
  itself generated. If inbound links point at one, it takes a host rule.
]

== Feeds

Feed files are named by format: `rss.xml`, `atom.xml`, `feed.json`. Zola's
default `atom.xml` matches; Hugo's `index.xml` and Jekyll's `feed.xml` do not,
and a subscriber's reader fetches the file rather than following a meta refresh.
Say what yours is called:

```kdl
generate {
  feed {
    formats "rss"
    names { rss "index.xml" }
  }
}
```

The file, the feed's own `<id>` and every page's autodiscovery tag follow the
name together.

== Check the result

Build both sites and compare the URLs they claim:

```sh
grep -o '<loc>[^<]*' old/sitemap.xml | sed 's/<loc>//' | sort > old-urls.txt
grep -o '<loc>[^<]*' public/sitemap.xml | sed 's/<loc>//' | sort > new-urls.txt
diff old-urls.txt new-urls.txt
```

Every line that only appears in `old-urls.txt` needs a `redirect` or a permalink
fix. `baudelaire check` covers the other half, the links inside your own pages.
