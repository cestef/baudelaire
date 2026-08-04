#let frontmatter = (
  title: "From Jekyll or Eleventy",
  order: 7,
)
#import "/templates/theme.typ": callout

Both sites are a content tree, a layout directory and a template language with
filters. Both also lean on plugins for what baudelaire has as config switches, so
the config usually gets shorter and the templates get rewritten.

Start with #link("content.typ")[Markdown to Typst] for the posts themselves, and
#link("urls.typ")[keeping your URLs] before you touch permalinks. The rest is
below.

== Jekyll: the tree

#table(
  columns: 3,
  align: (left, left, left),
  table.header([Jekyll], [Baudelaire], [Note]),
  [`_config.yml`], [`config.kdl`], [Every key is checked.],
  [`_posts/2026-07-09-hello.md`], [`content/posts/hello.typ`], [The date moves into frontmatter.],
  [`_drafts/`], [`draft: true`, or `hello.draft.typ`], [Built with `--drafts`.],
  [`_layouts/`], [`templates/`], [Liquid becomes Typst functions.],
  [`_includes/`], [any `.typ` file you import], [No magic directory.],
  [`_data/`], [any directory], [Read with `yaml()`, `json()`, `csv()`.],
  [`_sass/`], [`assets/`], [No Sass step; run one from `hooks { before }`.],
  [`assets/`], [`assets/`], [Bundled, minified and fingerprinted here.],
  [`_site/`], [`dist/`], [Renamed with `paths { dist }`.],
)

#callout(kind: "warn")[
  Jekyll strips the date out of `2026-07-09-hello.md`. Baudelaire does not, so
  that file would publish at `/posts/2026-07-09-hello/`. Rename the files, or set
  `slug` in each one, before setting the collection's `permalink`.
]

== Jekyll: the config

#table(
  columns: 2,
  align: (left, left),
  table.header([`_config.yml`], [`config.kdl`]),
  [`url` + `baseurl`], [`url`, path included],
  [`title`], [`site`],
  [`description`], [`description`],
  [`permalink: /:year/:month/:title/`], [`permalink "/{year}/{month}/{slug}/"` on the collection],
  [`paginate`, `paginate_path`], [`paginate { size }`, `paginate { prefix }`],
  [`collections`], [`content { collections { } }`],
  [`defaults`], [collection keys: `template`, `sort`, `permalink`],
  [`exclude`], [collection globs, or keep the file out of `content/`],
  [`sass`], [none; `hooks { before }`],
  [`theme`, `remote_theme`], [`theme`, pointing at a directory],
)

Most of what a Jekyll site installs is already here:

#table(
  columns: 2,
  align: (left, left),
  table.header([Plugin], [Config]),
  [`jekyll-feed`], [`generate { feed { formats "rss" "atom" } }`],
  [`jekyll-sitemap`], [`generate { sitemap #true }`],
  [`jekyll-seo-tag`], [`html { meta }` and `html { jsonld }`],
  [`jekyll-paginate`], [`paginate { size }`],
  [`jekyll-redirect-from`], [the `redirect` frontmatter key],
  [`jekyll-archives`], [`content { taxonomies { } }` with `listing=#true`],
)

== Jekyll: frontmatter

```yaml
---
layout: post
title: "Hello"
date: 2026-07-09
categories: [rust, cli]
published: false
redirect_from: ["/old/hello/"]
excerpt: "A short summary."
---
```

becomes

```typ
#let frontmatter = (
  template: "post.typ",
  title: "Hello",
  date: datetime(year: 2026, month: 7, day: 9),
  categories: ("rust", "cli"),
  draft: true,
  redirect: ("/old/hello/",),
  description: "A short summary.",
)
```

#table(
  columns: 2,
  align: (left, left),
  table.header([Jekyll], [Baudelaire]),
  [`layout`], [`template`, naming a file (`post.typ`)],
  [`published: false`], [`draft: true`],
  [`excerpt`], [`description`, written rather than extracted],
  [`redirect_from`], [`redirect`],
  [`categories`, `tags`], [top-level lists, one per declared taxonomy],
  [`permalink`], [`path`, taking a literal URL rather than a pattern. `/2019/post.html` publishes as that file.],
)

Jekyll can put categories in a URL (`/:categories/:year/:month/:title/`), and a
permalink here has no taxonomy token. What reproduces those URLs exactly is a
directory per category and `{path}`, which fills with the directories a page sits
under:

```kdl
content {
  collections {
    travel  { sort "date"; reverse #true; permalink "/{path}/{year}/{month}/{slug}/"; template "post.typ" }
    history { sort "date"; reverse #true; permalink "/{path}/{year}/{month}/{slug}/"; template "post.typ" }
  }
}
```

`content/travel/night-train.typ` then publishes at
`/travel/2026/03/night-train/`, as before. The cost is one collection per
category and a home-page listing that filters across them
(`pages(page.lang).filter(p => p.collection in ("travel", "history"))`). Keep
`categories` in frontmatter as well if you want the term listings.

A post in two categories cannot use it, since a file lives in one directory. Give
those posts a frontmatter `path` naming the URL they had, which beats the
collection's pattern outright.

== Eleventy: the tree

#table(
  columns: 3,
  align: (left, left, left),
  table.header([Eleventy], [Baudelaire], [Note]),
  [`eleventy.config.js`], [`config.kdl`], [Data, not code. Hooks run commands.],
  [`_includes/`], [`templates/`], [Layouts and partials both.],
  [`_data/`], [any directory], [No data cascade; a template reads what it needs.],
  [`addPassthroughCopy`], [`static/`], [Copied verbatim.],
  [`addFilter`, `addShortcode`], [`#let` functions in a file you import], [No registration.],
  [`collections.post`], [a collection block, plus `pages()` in a template], [Membership is a glob, not a tag.],
  [`_site/`], [`dist/`], [],
)

The data cascade is the piece with no counterpart. Eleventy merges global data,
directory data files and frontmatter into one object per page. Here a page has
its own frontmatter and nothing else, and the shared parts arrive explicitly:
collection defaults for `template`, `sort` and `permalink`, `client { }` or
`typst { inputs }` for constants, and an ordinary `yaml("/data/x.yaml")` read for
data. What a template reads is visible in the template.

One more with no counterpart: `pagination` over arbitrary data, which here is
always over a collection. A per-page `permalink` does carry over, as `path`,
though it takes a literal URL rather than a pattern.

== Liquid and Nunjucks, in Typst

```html
<!-- _layouts/post.html -->
<article>
  <h1>{{ page.title }}</h1>
  {% include byline.html %}
  {{ content }}
</article>
```

```typ
// templates/post.typ
#import "@baudelaire/html:0.1.0": h
#import "/templates/parts.typ": byline

#let post(page, body) = h("article")[
  #h("h1", page.frontmatter.title)
  #byline(page)
  #body
]
```

#table(
  columns: 2,
  align: (left, left),
  table.header([Liquid / Nunjucks], [Typst]),
  [`{{ page.title }}`], [`page.frontmatter.title`],
  [`{{ content }}`], [`body`],
  [`{{ page.hero }}`], [`page.frontmatter.at("hero", default: none)`],
  [`{% for post in site.posts %}`], [`for p in pages(page.lang).filter(p => p.collection == "posts")`],
  [`{% for post in paginator.posts %}`], [`for entry in page.frontmatter.entries` in a listing template],
  [`{% include x.html %}`], [`#import "/templates/parts.typ": x` then `#x(..)`],
  [`{% if %}`, `{% unless %}`], [`#if`, `#if not`],
  [`| date: "%Y"`], [`page.date.display`, or `datetime` formatting],
  [`| where`, `| sort`], [`.filter(..)`, `.sorted(key: ..)`],
  [`{{ site.title }}`], [`title` from `@baudelaire/site`],
  [`{{ site.data.authors }}`], [`yaml("/data/authors.yaml")`],
  [`{{ page.url }}`], [not available: a page does not know its own URL],
  [`{% post_url 2026-01-02-sleepers %}`], [`#link("sleepers.typ")`. Pandoc leaves the Liquid tag as text, so the link is quietly lost in conversion.],
  [`relative_url`], [nothing: a path under a subdirectory `url` is rewritten for you],
  [a layout chain (`layout:` on a layout)], [a function calling another function],
)

`page.url` is missing on purpose. Nothing site-wide may enter a page's wrapper,
or every page's cache identity would depend on every other page; the site tree
and the page catalogue come from `@baudelaire/sections` and `@baudelaire/pages`
instead. See #link("../../write/templates.typ")[templates].

== Commands

#table(
  columns: 2,
  align: (left, left),
  table.header([Jekyll / Eleventy], [Baudelaire]),
  [`jekyll build`, `eleventy`], [`baudelaire build`],
  [`jekyll serve`, `eleventy --serve`], [`baudelaire serve`],
  [`eleventy --incremental`], [on by default; `cache { incremental #false }` turns it off],
)

No Ruby, no Node, no lockfile: the Typst compiler, the bundler and the deploy
client are all in the one binary. If you were on GitHub Pages' own Jekyll build,
you now need a workflow that runs baudelaire; see
#link("../../ship/hosts/github-pages.typ")[GitHub Pages].

== Order of work

+ `baudelaire init` beside the old site, then copy the asset tree across. Sass keeps working through `hooks { before "sass --load-path=sass assets/style.scss assets/style.css" }`, but move the `.scss` out of `paths { assets }` first: anything in that tree is published, sources included.
+ Rename the dated post filenames, or give each a `slug`, then set the collection's `permalink` to the old shape.
+ Convert one section with #link("content.typ")[pandoc] and rewrite the layouts it needs.
+ Replace each plugin with its switch from the table above.
+ `baudelaire check`, then compare the old and new sitemaps.
