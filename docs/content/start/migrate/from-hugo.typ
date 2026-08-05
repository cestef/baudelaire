#let frontmatter = (
  title: "From Hugo",
  order: 6,
)
#import "/templates/theme.typ": callout

Hugo has the larger feature surface, so this is the migration where you should
check what you actually use. The posts themselves copy across as files: a `.md`
file under `content/` is a page here too, and its frontmatter block is read in
the language its fence already says it is. `---` is YAML, `+++` is TOML. What
has to change is a handful of frontmatter *key names*, the shortcodes, the
internal links and the config. Go templates and render hooks collapse into
ordinary Typst functions.

#callout(kind: "note")[
  Converting a post to `.typ` is a choice, not a step. Make it when you want what
  Typst gives you: math, a figure, a chart, a template helper mid-page. See
  #link("../../write/markdown.typ")[Markdown pages] for what a `.md` page can
  carry, and #link("content.typ")[Markdown to Typst] for the conversion and its
  pandoc pass.
]

== The tree

#table(
  columns: 3,
  align: (left, left, left),
  table.header([Hugo], [Baudelaire], [Note]),
  [`hugo.toml`], [`config.kdl`], [One file, no `config/_default/` layering. Use `profiles { }` for per-environment overrides.],
  [`content/**/*.md`], [`content/**/*.md`], [Copied as-is, `---` or `+++` block included. Some key names change; see below.],
  [`content/posts/_index.md`], [a `posts` block in `config.kdl`], [No branch-bundle file. Nothing reads a leading `_`: left in place the file is an ordinary page at `/posts/index/`. Rename it to `content/posts/index.md` and it is the page at `/posts/`.],
  [`content/_index.md`], [`content/index.md`], [The home page either way: a stem of `index` under `content/` publishes at `/`, `_` or no `_`.],
  [`content/posts/hello/index.md`], [`content/posts/hello/index.md`], [Leaf bundles work the same way. `index` is the stem, whatever the extension; `content { index }` renames it.],
  [`layouts/_default/baseof.html`], [a function every layout calls], [Composition, not inheritance.],
  [`layouts/_default/single.html`], [`templates/page.typ`], [Bound by name in the config.],
  [`layouts/_default/list.html`], [`templates/list.typ`], [Bound under `paginate`.],
  [`layouts/partials/`], [any `.typ` file you import], [No magic directory.],
  [`layouts/shortcodes/`], [any `.typ` file you import], [No registry. Called from an `eval` fence, or directly on a `.typ` page.],
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
  [`markup.goldmark.extensions`], [`content { markdown { extensions } }`],
  [`markup.goldmark.renderer.unsafe`], [`content { markdown { html } }`, and there is no `#true`: see below],
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

Goldmark's defaults and this parser's are close but not identical. Tables,
footnotes, strikethrough and tasklists are on; Hugo's typographer is not, and is
`extensions "smart"`. The full list is in the
#link("../../configure/reference.typ")[reference] under
`content.markdown.extensions`, where a `*` marks what you already have.

== Frontmatter

The block does not change language. The fence picks it, so a Hugo post's `---`
is read as YAML and a `+++` as TOML, exactly as Hugo read them. What changes is
the spelling of a few keys:

```yaml
---
title: "Hello"
date: 2026-07-09
lastmod: 2026-08-01
draft: true
weight: 3
aliases: ["/old/hello/", "/older/hello/"]
tags: ["rust", "cli"]
params:
  hero: cover.png
---
```

becomes

```yaml
---
title: "Hello"
date: 2026-07-09
updated: 2026-08-01
draft: true
order: 3
redirect: ["/old/hello/", "/older/hello/"]
tags: ["rust", "cli"]
params:
  hero: cover.png
---
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
  [`layout`, `type`], [`template`, naming a file in `templates/`],
  [`tags`, `categories`], [the same names, once `content { taxonomies { } }` declares them],
  [`params:` nested keys], [unchanged; a template reads `page.frontmatter.params.hero`],
  [`url`], [`path`: the exact URL this page publishes at],
  [`post.fr.md`, `translationKey`], [`post.fr.md` or an explicit `lang`, and `translation`],
  [`headless`], [none],
)

#callout(kind: "warn")[
  *A key nobody recognises is not an error.* It lands in `page.frontmatter` for a
  template to read, and no built-in behaviour attaches to it: a `weight` left
  alone sorts nothing, an `aliases` left alone publishes no redirect, a `url`
  left alone moves no page. Only a key within an edit or two of a real one is
  caught as a typo, and none of Hugo's are. So the renames in the table are the
  whole risk of the copy: do them first, then grep the tree for the old spellings.
]

#callout(kind: "warn")[
  *A date is the bare ISO day.* `hugo new` writes `date: 2026-07-09T10:00:00+02:00`,
  and that fails the build with *a string that is not an ISO day*. Cut it to
  `2026-07-09`. TOML's date literal (`date = 2026-07-09`) is accepted and reaches
  the same reader; a TOML datetime does not.
]

A `tags` key is nothing at all until the taxonomy is declared. Until then it is
an unrecognised key like any other, so a copied post publishes with no terms and
no complaint. See #link("../../write/collections/taxonomies.typ")[taxonomies].

The third fence, `;;;`, is KDL, the language the config is written in. Nothing
about a Hugo migration needs it, and it is the one dialect that cannot spell a
one-element list. See #link("../../write/markdown.typ")[Markdown pages].

A collection can require the fields its template reads, which is the checked
version of what Hugo leaves to `.Params` and an empty string. See
#link("../../write/frontmatter.typ")[schemas].

#callout(kind: "warn")[
  Hugo's `.Summary` is generated from the first paragraphs or a `<!--more-->`
  marker. Nothing here does that: write a `description`, which then fills the
  meta tag, the feed entry, the search index and the social card at once. A
  listing template reads it back as `entry.description`, and `summary` is
  accepted as a spelling of the same key.
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

== Raw HTML

Hugo's Goldmark passes HTML through once `renderer.unsafe` is on, and themes lean
on it. Here a `.md` page that contains any is refused, because the DOM a build
produces is typed and a string of markup has nowhere to be spliced into it. Three
answers, in the order worth trying:

#table(
  columns: 2,
  align: (left, left),
  table.header([Do], [Result]),
  [write the element in a `typ eval` fence], [`#html.elem("aside")[..]`, and it goes through the same typed DOM as everything else],
  [`content { markdown { html "drop" } }`], [an inline run loses its tags and keeps the prose between them; a block-level run is dropped whole, contents included],
  [convert the page to `.typ`], [`h("div", class: "x")[..]`, from `@baudelaire/html`],
)

The refusal names the file, the line and the tag, so a tree is worked through by
building it. `html "drop"` is the one to be careful with: an embedded widget is a
block-level run, and dropping it drops what it contained.

== Shortcodes and render hooks

Both become Typst functions. A shortcode lives in a file the page or template
imports:

```typ
// templates/parts.typ
#let youtube(id) = html.elem("iframe", attrs: (
  src: "https://www.youtube-nocookie.com/embed/" + id,
  loading: "lazy",
))
```

A markdown page reaches it through an `eval` fence, which is where
`{{< youtube id >}}` used to be:

````md
```typ eval
#import "/templates/parts.typ": youtube
#youtube("dQw4w9WgXcQ")
```
````

A `.typ` page calls `#youtube("dQw4w9WgXcQ")` outright. Either way `eval #false`
turns the fences off for a site that imports content it did not write.

A render hook, which in Hugo rewrites every link or image in the Markdown, is a
`#show` rule in the template that draws the page, so it applies to a `.md` body
the same as a `.typ` one:

```typ
#let page(page, body) = {
  show link: it => h("span", class: "external", it)
  body
}
```

#callout(kind: "warn")[
  A shortcode left in a page is not an error. `{{< youtube .. >}}` and
  `{{% notice %}}` are ordinary text to a markdown parser, so they publish as
  text on a green build. Grep for `{{<` and `{{%` before you ship.
]

== Internal links

`{{< relref "sleepers.md" >}}` and `{{< ref >}}` have no meaning here, and no
error either: they are the text above. What they wrapped is what you keep.

```md
[sleepers](sleepers.md)
```

A plain link naming a source path is resolved against the linking file and
rewritten to that page's permalink, and if no page is there the build fails.
Both extensions work and either may name either kind of page, so a `.md` post
linking a converted `.typ` one needs no thought. That is the mechanism that
finds the links a migration broke.

#table(
  columns: 3,
  align: (left, left, left),
  table.header([Href], [Published as], [Checked]),
  [`sleepers.md`, `sleepers.typ`, `/posts/a.md`], [that page's permalink], [yes; a trailing `#part` is checked against that page's headings too],
  [`/posts/sleepers/`], [itself], [no],
  [`https://..`, `mailto:`, `#anchor`], [itself], [no; external links have a pass of their own],
)

So convert `{{< relref >}}` to the source path, not to the URL it produced: a URL
survives a rename only if the URL does. `links { strict #false }` demotes the
failure to a warning, which is the setting for working through a large tree. See
#link("../../write/pages.typ")[pages].

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
  [render hooks], [a `#show` rule in the template],
)

The absent `.Permalink` is deliberate: nothing site-wide may go into a page's
wrapper, or every page's cache identity would depend on every other page. The
site tree and the page catalogue come from `@baudelaire/sections` and
`@baudelaire/pages`, which typst tracks as files. See
#link("../../write/templates.typ")[templates].

== Page resources

A Hugo page bundle addresses its own files. A bundle here is a directory with an
`index.md` or `index.typ` in it, and the same files are reached two ways:

#table(
  columns: 2,
  align: (left, left),
  table.header([Hugo], [Baudelaire]),
  [`.Resources.GetMatch "cover.png"`], [`page.assets.at("cover.png")`, in the template],
  [a `hero` param naming a file beside the page], [the same, resolved through `page.assets`],
  [`.Resources.ByType "image"`], [none; `page.assets` is a dict, filter it yourself],
  [`{{ $img.Resize "600x" }}`], [`assets { images { responsive } }`, applied to every extracted image],
)

The body names it the way Markdown always did:

```md
![Vienna](vienna.png)
```

The picture publishes at `/assets/posts/hello/vienna.png`: an extracted image
keeps the directories it was authored under, relative to `content/`, so a tree
where every post has its own `cover.png` does not collide. Only a file the page
actually shows is written, so a `hero` the template draws from `page.assets` has
to be named in the body as well, or it has a URL and no file. See
#link("../../build/images.typ")[images].

== Commands

#table(
  columns: 2,
  align: (left, left),
  table.header([Hugo], [Baudelaire]),
  [`hugo`], [`baudelaire build`],
  [`hugo server`], [`baudelaire serve`],
  [`hugo new content posts/hello.md`], [`baudelaire new posts/hello`, which scaffolds a `.typ`],
  [`hugo deploy`], [`baudelaire deploy`],
  [`hugo --minify`], [`assets { minify }`],
)

== Order of work

+ `baudelaire init` beside the old site, then copy `static/`, `assets/` and `content/` across.
+ Port the config. An unknown key fails the build with a suggestion, so work down the file until it is quiet.
+ Rename the frontmatter keys in one section, and set its collection's `permalink` to whatever `permalinks` produced before. See #link("urls.typ")[keeping your URLs].
+ Grep that section for `{{<`, `{{%` and raw HTML: those are what a page cannot carry across silently.
+ Rewrite the templates it needs, starting with the shell.
+ `baudelaire check`, then compare the old and new sitemaps.

#callout(kind: "note")[
  Other things Hugo has that have no counterpart: modules, `.Summary`, related
  content, taxonomy term pages with their own content files, `cascade`, output
  formats beyond the generated ones, and the whole `resources` chain except
  bundling, minifying, fingerprinting and image variants. Hugo also writes
  `/posts/page/1/` and a `page/1` under every term; nothing here does, and a
  top-level `redirect { }` pair is what claims those paths back. Check what your
  theme relies on before you start.
]
