#let frontmatter = (
  title: "From Hugo",
  order: 6,
)
#import "/templates/theme.typ": callout

Hugo has the larger feature surface, so this is the migration where you should
check what you actually use. The content tree, the taxonomies and the asset
pipeline all have a counterpart. Go templates, shortcodes and render hooks all
collapse into ordinary Typst functions.

== The tree

#table(
  columns: 3,
  align: (left, left, left),
  table.header([Hugo], [Baudelaire], [Note]),
  [`hugo.toml`], [`config.kdl`], [One file, no `config/_default/` layering. Use `profiles { }` for per-environment overrides.],
  [`content/**/*.md`], [`content/**/*.md`], [Stays markdown; the frontmatter becomes KDL. See #link("content.typ")[Markdown to Typst].],
  [`content/posts/_index.md`], [a `posts` block in `config.kdl`], [No branch-bundle file.],
  [`content/posts/hello/index.md`], [`content/posts/hello/index.typ`], [Page bundles work the same way.],
  [`layouts/_default/baseof.html`], [a function every layout calls], [Composition, not inheritance.],
  [`layouts/_default/single.html`], [`templates/page.typ`], [Bound by name in the config.],
  [`layouts/_default/list.html`], [`templates/list.typ`], [Bound under `paginate`.],
  [`layouts/partials/`], [any `.typ` file you import], [No magic directory.],
  [`layouts/shortcodes/`], [any `.typ` file you import], [No registry.],
  [`assets/`], [`assets/`], [Same idea, fewer knobs.],
  [`static/`], [`static/`], [Copied verbatim.],
  [`data/`], [any directory], [Read with `json()`, `yaml()`, `toml()`, `csv()` from the page.],
  [`i18n/*.toml`], [`languages { fr { strings { .. } } }`], [In the config.],
  [`archetypes/`], [`baudelaire new`], [The scaffold is built in, not templated.],
  [`public/`], [`dist/`, renamed with `paths { dist }`], [],
)

== The config

#table(
  columns: 2,
  align: (left, left),
  table.header([Hugo], [Baudelaire]),
  [`baseURL`], [`url`],
  [`title`], [`site`],
  [`languageCode`, `defaultContentLanguage`], [`lang`],
  [`languages`], [`languages { }`],
  [`theme`], [`theme`],
  [`publishDir`], [`paths { dist }`],
  [`taxonomies`], [`content { taxonomies { } }`],
  [`permalinks`], [`permalink` on the collection],
  [`pagination.pagerSize`, older `paginate`], [`paginate { size }` on the collection],
  [`outputs` with `RSS`], [`generate { feed { formats } }`; per section, `feed #true` on the collection],
  [`enableRobotsTXT`], [`generate { robots { } }`],
  [`sitemap`], [`generate { sitemap #true }`],
  [`minify`], [`assets { minify }` for CSS and JS; HTML is not minified],
  [`buildDrafts`, `buildFuture`, `buildExpired`], [`--drafts`, `--future`, and an `expiry` date that is final],
  [`markup.highlight`], [`html { highlight }`, which maps compiler colours to classes],
  [`params`], [`client { }` for the browser, `typst { inputs }` for the compiler],
  [`module`], [none],
  [`cascade`], [collection defaults: `template`, `sort`, `permalink`],
)

Hugo infers a section from a directory and layers `_index.md`, `type` and
`cascade` on top. Here a directory is a collection, and everything about it is in
one block:

```kdl
content {
  collections {
    posts {
      sort "date"
      reverse #true
      permalink "/{year}/{month}/{slug}/"
      template "page.typ"
      paginate { size 10; template "list.typ" }
    }
  }
}
```

`_root { template "page.typ" }` covers the pages directly under `content/`, which
is the closest thing to Hugo's home and top-level singles.

== Frontmatter

```yaml
---
title: "Hello"
date: 2026-07-09
lastmod: 2026-08-01
draft: true
weight: 3
aliases: ["/old/hello/"]
tags: ["rust"]
params:
  hero: cover.png
---
```

becomes

```typ
#let frontmatter = (
  title: "Hello",
  date: datetime(year: 2026, month: 7, day: 9),
  updated: datetime(year: 2026, month: 8, day: 1),
  draft: true,
  order: 3,
  redirect: ("/old/hello/",),
  tags: ("rust",),
  hero: "cover.png",
)
```

#table(
  columns: 2,
  align: (left, left),
  table.header([Hugo], [Baudelaire]),
  [`title`, `date`, `draft`, `slug`, `description`], [the same names],
  [`lastmod`], [`updated`],
  [`expiryDate`], [`expiry`],
  [`publishDate`], [`date`],
  [`weight`], [`order`],
  [`aliases`], [`redirect`],
  [`layout`, `type`], [`template`],
  [`tags`, `categories`], [top-level lists, one per declared taxonomy],
  [`params:` nested keys], [top-level keys, read as `page.frontmatter.<key>`],
  [`url`], [`path`: the exact URL this page publishes at.],
  [`headless`], [none],
)

A collection can require the fields its template reads, which is the checked
version of what Hugo leaves to `.Params` and an empty string. See
#link("../../write/frontmatter.typ")[schemas].

#callout(kind: "warn")[
  Hugo's `.Summary` is generated from the first paragraphs or a `<!--more-->`
  marker. Nothing here does that: write a `description`, which then fills the
  meta tag, the feed entry, the search index and the social card at once. A
  listing template reads it back as `entry.extra.description`.
]

=== The slug is not the same string

This is the one that quietly rewrites every URL. Hugo's `:slug` token falls back
to the *title*, urlized. Baudelaire's slug comes from the *filename*.

#table(
  columns: 3,
  align: (left, left, left),
  table.header([File], [Hugo, under `/:year/:month/:slug/`], [Here, unchanged]),
  [`posts/tickets.md`, titled "Buying tickets without an app"],
  [`/2025/11/buying-tickets-without-an-app/`],
  [`/2025/11/tickets/`],
)

So a Hugo site whose filenames are short and whose titles are sentences moves
every post the moment you copy the permalink pattern across. Either rename the
files to the old slugs, or write `slug` into each page's frontmatter, before
comparing sitemaps. `slug` in Hugo frontmatter already pinned the URL and carries
over unchanged.

== Templates

```html
<!-- layouts/_default/single.html -->
{{ define "main" }}
  <h1>{{ .Title }}</h1>
  {{ partial "byline.html" . }}
  {{ .Content }}
{{ end }}
```

```typ
// templates/page.typ
#import "@baudelaire/html:0.1.0": h
#import "/templates/parts.typ": shell, byline

#let page(page, body) = shell(page)[
  #h("h1", page.frontmatter.title)
  #byline(page)
  #body
]
```

#table(
  columns: 2,
  align: (left, left),
  table.header([Go template], [Typst]),
  [`{{ .Title }}`], [`page.frontmatter.title`],
  [`{{ .Content }}`], [`body`],
  [`{{ .Params.hero }}`], [`page.frontmatter.at("hero", default: none)`],
  [`{{ .Date }}`], [`page.date.display` (and `page.date.iso`)],
  [`{{ range .Pages }}`], [`for p in pages(page.lang).filter(p => p.collection == "posts")`],
  [`{{ range .Paginator.Pages }}`], [`for entry in page.frontmatter.entries` in a listing template],
  [`{{ .Next }}`, `{{ .Prev }}`], [`page.nav.next`, `page.nav.prev`],
  [`{{ partial "x.html" . }}`], [`#import "/templates/parts.typ": x` then `#x(page)`],
  [`{{ define }}` / `{{ block }}`], [a function that takes the body],
  [`{{ .Site.Title }}`], [`title` from `@baudelaire/site`],
  [`{{ .Site.Params.x }}`], [a `client { }` constant],
  [`{{ .Site.Data.authors }}`], [`yaml("/data/authors.yaml")`],
  [`{{ i18n "readMore" }}`], [`page.strings.at("read-more", default: "Read more")`],
  [`{{ .Permalink }}`], [not available: a page does not know its own URL],
  [`resources.Get | toCSS`], [none; run Sass from `hooks { before }`],
  [`js.Build`], [`assets { bundle }`],
  [`| fingerprint`], [`assets { fingerprint }` and `security { sri }`],
  [`.Resize "600x"`], [`assets { images { responsive { widths .. } } }`],
  [render hooks], [Typst `#show` rules in the page or a template],
)

The absent `.Permalink` is deliberate: nothing site-wide may go into a page's
wrapper, or every page's cache identity would depend on every other page. The
site tree and the page catalogue come from `@baudelaire/sections` and
`@baudelaire/pages`, which typst tracks as files. See
#link("../../write/templates.typ")[templates].

== Shortcodes and render hooks

Both become functions. A shortcode is called where you would have written
`{{< youtube id >}}`:

```typ
#import "/templates/parts.typ": youtube
#youtube("dQw4w9WgXcQ")
```

A render hook, which in Hugo rewrites every link or image in the Markdown, is a
Typst show rule, and it can live in one file that every page imports:

```typ
#show link: it => text(fill: blue, it)
```

`{{< relref "sleepers.md" >}}` and `{{< ref >}}` do not survive the conversion at
all: pandoc leaves the whole link as escaped text, so it stops being a link
without stopping the build. Rewrite each as `#link("sleepers.typ")`, which is
checked.

== Commands

#table(
  columns: 2,
  align: (left, left),
  table.header([Hugo], [Baudelaire]),
  [`hugo`], [`baudelaire build`],
  [`hugo server`], [`baudelaire serve`],
  [`hugo new content posts/hello.md`], [`baudelaire new posts/hello`],
  [`hugo deploy`], [`baudelaire deploy`],
  [`hugo --minify`], [`assets { minify }`],
)

== Order of work

+ `baudelaire init` beside the old site, then copy `static/` and `assets/` across.
+ Port the config. An unknown key fails the build with a suggestion, so work down the file until it is quiet.
+ Convert one section, with #link("content.typ")[pandoc], and set its collection's `permalink` to whatever `permalinks` produced before. See #link("urls.typ")[keeping your URLs].
+ Rewrite the templates that section needs, starting with the shell.
+ `baudelaire check`, then compare the old and new sitemaps.

== Page resources

A Hugo page bundle addresses its own files: `.Resources.GetMatch "vienna.png"`,
`.Resources.ByType "image"`, a `hero` param naming a file beside the page. None of
that exists here. A bundle is just a directory with an `index.typ` in it, and the
only way to reach a colocated file is to name it in the page body:

```typ
#image("vienna.png", alt: "")
```

Two consequences worth knowing before you copy a bundle tree across. The image
publishes to `/assets/vienna.png`, not beside the page, so every bundle shares
one flat namespace and a tree of `cover.png` files collides (a warning per clash,
and one file served to all of them; `assets { fingerprint }` names them by
content instead). And a template cannot resolve a frontmatter `hero`, because a
page does not know its own path: give it an absolute asset path, or draw the hero
in the body.

#callout(kind: "note")[
  Other things Hugo has that have no counterpart: modules, `.Summary`, related
  content, taxonomy term pages with their own content files, `cascade`, output
  formats beyond the generated ones, and the whole `resources` chain except
  bundling, minifying, fingerprinting and image variants. Hugo also writes
  `/posts/page/1/` and a `page/1` under every term; nothing here does, and a
  top-level `redirect { }` pair is what claims those paths back. Check what your
  theme relies on before you start.
]
