#let frontmatter = (
  title: "Keeping your URLs",
  order: 9,
)
#import "/templates/theme.typ": callout

Changing paths during a migration breaks inbound links, search rankings and
bookmarks. Most old URL shapes need one `permalink` line; the rest need
redirects.

== Set the URL on the page

The simplest option: a page names the URL it answers at.

```typ
#let frontmatter = (
  title: "The night train to Vienna",
  path: "/2019/03/night-train.html",
)
```

A converter can copy each old URL onto the page that answers it, whatever the
old site's permalink rules were, and the URL set then matches. A path ending in
a file name publishes as that file, which is the shape a Jekyll site uses. See #link("../../write/frontmatter.typ")[frontmatter].

When the old URLs follow a rule, use the patterns below instead: one line per
collection replaces one line per page, and new pages get the same shape.

== Default permalink

`/{path}/{slug}/`: the directories a page sits under, then its slug. A collection
sets its own with #link("../../collections/defining.typ")[`permalink`],
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
  Jekyll takes the date from the filename, baudelaire does not.
  `2026-07-09-hello.md` becomes the slug `2026-07-09-hello` unless you rename the
  file or set `slug: "hello"` in its frontmatter. Do this before setting the permalink, or
  the date token ends up next to a date already in the slug.
]

== Listings

#table(
  columns: 2,
  align: (left, left),
  table.header([URL], [Comes from]),
  [`/blog/`], [`paginate { }` on the `blog` collection.],
  [`/blog/page/2/`], [`paginate { size 5 }`. `prefix` renames `page`, `mount` moves page 1.],
  [`/tags/`, `/tags/rust/`], [`taxonomies { tags { listing } }`.],
)

A taxonomy publishes under its own id, so the id is the URL segment. To serve
Hugo's `categories` at `/topics/`, name the taxonomy `topics` and point it at the
old frontmatter key:

```kdl
content {
  taxonomies {
    topics { key "categories"; listing { template "list.typ" } }
  }
}
```

== Pages that moved

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
redirects {
  file #true
}
```

Both are on #link("../../collections/redirects.typ")[redirects], including
why the two cannot be served at once.

`redirect` is a frontmatter key, so only a page that still exists can claim an
old path that way. Zola's `/blog/page/1/`, Hugo's `page/1` under every term, and
a section you dropped on the way over have no source file to declare anything
in. Claim those in the config, old path first:

```kdl
redirects {
  rules {
    "/blog/page/1/" "/blog/"
    "/tags/rs/" "/tags/rust/"
  }
}
```

Same output, same `redirects { file }` switch. A pair aiming at a path some
page already publishes fails the build.

== Feeds

Feed files are named by format: `rss.xml`, `atom.xml`, `feed.json`. Zola's
default `atom.xml` matches; Hugo's `index.xml` and Jekyll's `feed.xml` do not,
and a subscriber's reader fetches the file without following a meta refresh. Set
the name:

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
fix. `baudelaire check` covers the links inside your own pages.
