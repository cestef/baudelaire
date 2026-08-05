#let frontmatter = (
  title: "From Jekyll or Eleventy",
  order: 7,
)
#import "/templates/theme.typ": callout

Both sites are a content tree, a layout directory and a template language with
filters. Both also lean on plugins for what baudelaire has as config switches, so
the config usually gets shorter and the templates get rewritten.

#callout(kind: "note")[
  *A post's frontmatter needs no conversion.* A `.md` file under `content/` is a
  page (#link("../../write/markdown.typ")[Markdown pages]), and a `---` block is
  read as YAML, exactly as Jekyll reads it. The file copies across as it stands.
  What changes is the *key names* - `layout` is `template`, `published: false`
  is `draft: true` - not the language they are written in. Converting a post to
  Typst is a separate choice, made for what Typst gives you;
  #link("content.typ")[Markdown to Typst] is where that lives.
]

What is left is the frontmatter keys, the Liquid, the config and the layouts.
Read #link("urls.typ")[keeping your URLs] before you touch permalinks.

== Jekyll: the tree

#table(
  columns: 3,
  align: (left, left, left),
  table.header([Jekyll], [Baudelaire], [Note]),
  [`_config.yml`], [`config.kdl`], [KDL, and every key is checked. The config is the rewrite; the pages are not.],
  [`_posts/2026-07-09-hello.md`], [`content/posts/hello.md`], [The same file, YAML frontmatter and all. Only the keys are renamed.],
  [`_drafts/`], [`draft: true` in frontmatter, or `hello.draft.md`], [Built with `--drafts`.],
  [`_layouts/`], [`templates/`], [Liquid becomes Typst functions.],
  [`_includes/`], [any `.typ` file you import], [No magic directory.],
  [`_data/`], [any directory], [Read with `yaml()`, `json()`, `csv()`.],
  [`_sass/`], [`assets/`], [No Sass step; run one from `hooks { before }`.],
  [`assets/`], [`assets/`], [Bundled, minified and fingerprinted here.],
  [`_site/`], [`dist/`], [Renamed with `paths { dist }`.],
)

#callout(kind: "warn")[
  Jekyll reads the date out of `2026-07-09-hello.md` and drops it from the slug.
  Baudelaire reads neither: that file publishes at `/posts/2026-07-09-hello/`,
  and `{year}` in a permalink fills from the frontmatter `date` or from nothing
  at all. Rename the files, or set `slug` in each one, and make sure every post
  carries a `date` before setting the collection's `permalink`.
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
  [`markdown`, `kramdown`], [`content { markdown { extensions } }`. Kramdown's typographic quotes are `extensions "smart"`, which is off until asked for.],
  [`highlighter`], [`html { highlight }`, which maps compiler colours to classes],
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

`---` is YAML here as well, so the block parses as written. Only the key names
move:

```yaml
---
layout: post
title: "Hello"
date: 2026-07-09
categories: [rust, cli]
published: false
excerpt: "A short summary."
---
```

becomes, in the same file and the same language:

```yaml
---
template: post.typ
title: "Hello"
date: 2026-07-09
categories: [rust, cli]
draft: true
description: "A short summary."
---
```

`title`, `date` and `categories` did not have to move at all. An unquoted
`2026-07-09` reaches the date reader as the ISO day it looks like, quoted or
not, and `[rust, cli]` is a list of two - including a list of one, which YAML
writes without ceremony.

#table(
  columns: 2,
  align: (left, left),
  table.header([Jekyll], [Baudelaire]),
  [`layout`], [`template`, naming a file (`post.typ`)],
  [`published: false`], [`draft: true`],
  [`excerpt`], [`description` (or `summary`), written rather than extracted],
  [`redirect_from`], [`redirect`],
  [`categories`, `tags`], [top-level lists, one per declared taxonomy],
  [`permalink`], [`path`, taking a literal URL rather than a pattern. `/2019/post.html` publishes as that file.],
)

A key that is neither a built-in nor a declared taxonomy is kept as it is, and a
template reads it back with `page.frontmatter.at("author", default: none)`. A
key that is a *near-miss* of a real one fails the build naming what you probably
meant, which is what catches a rename left half-done.

#callout(kind: "note")[
  The fence picks the language, so `+++` is TOML and `;;;` is KDL, the one
  `config.kdl` is written in. Nothing else changes: the same keys, the same
  schema, the same errors. See #link("../../write/markdown.typ")[Markdown pages],
  and note that KDL cannot spell a one-element list.
]

On a `.typ` page the same keys are a Typst dict, and the types are the language's
own:

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

== Jekyll: Liquid in a post

Liquid is not markup here. A `{% .. %}` tag in a `.md` page is text, and stays
text: nothing strips it and nothing warns. Grep for `{%` and `{{` before you
build.

#table(
  columns: 2,
  align: (left, left),
  table.header([In a post], [Here]),
  [`{% post_url 2026-01-02-sleepers %}`], [an ordinary link, naming the file: `[..](sleepers.md)`.],
  [`{% link about.md %}`], [the same, and the `.md` path it already names works as written.],
  [`{% highlight ruby %} .. {% endhighlight %}`], [a fenced block, highlighted by the compiler.],
  [`{% include note.html %}`], [a `typ eval` fence, or the page converted to Typst.],
  [`{{ site.baseurl }}/x`], [`/x`: a path under a subdirectory `url` is rewritten for you.],
  [`<!--more-->`], [nothing. Write a `description`, which fills the meta tag, the feed entry, the search index and the card at once.],
  [raw HTML in a post], [refused: `content { markdown { html "drop" } }` drops it instead, or write the element in a `typ eval` fence.],
)

A `{% post_url %}` becomes an ordinary link, and which spelling you give it
decides whether the build checks it. A source path - `.md` or `.typ`, whichever
the target actually is - is rewritten to that page's permalink, and fails the
build when no page sits there:

```md
See [the sleepers](sleepers.md).
```

A URL (`/blog/sleepers/`) is passed through untouched, so it is neither checked
nor carried along by a later rename.

An include, if the page stays markdown, is an `eval` fence:

````md
```typ eval
#import "/templates/parts.typ": note
#note[The include you used to register.]
```
````

That fence runs arbitrary Typst at build time. A site building posts it did not
write should set `content { markdown { eval #false } }`.

== Jekyll: categories in the URL

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

`content/travel/night-train.md` then publishes at
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
  [`*.md`], [`*.md`], [Stays markdown, and its `---` or `+++` block stays what it was.],
  [`*.njk`, `*.liquid`, `*.11ty.js` as content], [none], [A page is `.typ` or `.md`. A template language is not a content format.],
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

== Layouts, in Typst

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
+ Copy `_posts/` into `content/posts/` unchanged - the YAML blocks parse as they are - then rename the keys from the table above. Rename the dated filenames, or give each a `slug`, then set the collection's `permalink` to the old shape.
+ Rewrite the layouts that section needs, starting with the shell.
+ Replace each plugin with its switch from the table above.
+ Grep the posts for `{%`, `{{` and `<`: Liquid is text now, and raw HTML is refused.
+ `baudelaire check`, then compare the old and new sitemaps.

Converting the prose itself is optional, and separate. When you want Typst for a
page - math, a figure, a template helper mid-page - #link("content.typ")[Markdown
to Typst] has the pandoc pass and what it drops.
