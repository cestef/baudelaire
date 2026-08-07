#let frontmatter = (
  title: "From Zola",
  order: 5,
)
#import "/templates/theme.typ": callout

The closest neighbour: one Rust binary, a content tree, a template directory,
taxonomies and pagination in the config. The posts stay markdown, since a `.md`
file under `content/` is a page (#link("../../write/markdown.typ")[Markdown
pages]), and a `+++` block is TOML here exactly as it is there, so a post copies
over as it stands. What has to change is a handful of frontmatter *key names*,
the shortcodes, the internal links and the config; the templates are a rewrite.

#callout(kind: "note")[
  Converting a post to `.typ` is a choice, not a migration step. Make it for the
  pages that want what Typst gives - math, a figure, a template helper mid-page -
  and leave the rest as they are. #link("content.typ")[Markdown to Typst] has the
  mapping and a pandoc walkthrough for when you do.
]

== The tree

#table(
  columns: 3,
  align: (left, left, left),
  table.header([Zola], [Baudelaire], [Note]),
  [`config.toml`], [`config.kdl`], [The one real rewrite: the config is KDL only, and every key is checked.],
  [`content/**/*.md`], [`content/**/*.md`], [Stays markdown, `+++` block included. Some key names change.],
  [`content/blog/_index.md`], [a `blog` block in `config.kdl`], [There is no per-section file, and a copied one publishes as a page.],
  [`content/blog/hello/index.md`], [the same], [A page bundle is a directory with an `index.md` in it.],
  [`templates/*.html`], [`templates/*.typ`], [Tera becomes Typst functions.],
  [`templates/shortcodes/`], [any `.typ` file you import], [No registry.],
  [`sass/`], [`assets/`], [No Sass step; see below.],
  [`static/`], [`static/`], [Copied verbatim, same as Zola.],
  [`themes/blow`], [`theme "themes/blow"`], [Different format; a theme is not portable.],
  [`public/`], [`public/`], [The same default; rename it with `paths { dist }`.],
)

#callout(kind: "warn")[
  Delete the `_index.md` files as you copy the tree. A `.md` file is a page, so
  `content/blog/_index.md` publishes at `/blog/index/`, and its block parses
  cleanly: `sort_by`, `paginate_by` and `transparent` are simply unrecognized
  keys and do nothing. Nothing fails, and nothing tells you the section is
  configured nowhere. Those settings become a collection block, below.
]

== The config

#table(
  columns: 2,
  align: (left, left),
  table.header([`config.toml`], [`config.kdl`]),
  [`base_url`], [`url`],
  [`title`], [`site`],
  [`description`], [`description`],
  [`default_language`], [`lang`],
  [`[languages.fr]`], [`languages { fr { .. } }`],
  [`theme`], [`theme`],
  [`output_dir`], [`paths { dist }`],
  [`taxonomies = [{ name = "tags" }]`], [`content { taxonomies { tags listing=#true } }`],
  [`generate_feeds`, `feed_filenames`], [`generate { feed { formats "atom" } }`],
  [`build_search_index`], [`generate { search { } }`],
  [`compile_sass`], [none; run Sass from `hooks { before }`],
  [`minify_html`], [none],
  [`[markdown] highlight_code`], [always on; `html { highlight }` maps the colours to classes],
  [`[markdown] smart_punctuation`], [`content { markdown { extensions "smart" } }`, off by default as it is there],
  [`[link_checker]`], [`links { external { } }`; `skip_prefixes` is `ignore`],
  [`[slugify]`], [fixed rules; override per page with `slug`],
  [`[extra]`], [`client { }` for the browser, `typst { inputs }` for the compiler],
)

Tables, footnotes, strikethrough and tasklists are on without asking, and the
list adds rather than replaces, so `extensions "smart"` keeps all four and turns
typographic quotes on. `-name` drops a default. The full list is in the
#link("../../configure/reference.typ")[reference] under `content.markdown.extensions`.

A Zola section carries its own settings in `_index.md`. Here they are a
collection block:

```kdl
content {
  collections {
    blog {
      sort "date"
      reverse #true
      template "page.typ"
      paginate { size 10; template "list.typ" }
    }
  }
}
```

#table(
  columns: 2,
  align: (left, left),
  table.header([`_index.md`], [collection key]),
  [`sort_by = "date"`], [`sort "date"` with `reverse #true`],
  [`sort_by = "weight"`], [`sort "order"`],
  [`paginate_by`], [`paginate { size }`],
  [`paginate_path`], [`paginate { prefix }`],
  [`template`], [`paginate { template }`],
  [`page_template`], [`template`],
  [`generate_feeds`], [`feed #true`, which writes it beside the section index],
  [`transparent`], [a `glob` reaching into the subdirectories],
  [`redirect_to`], [a `redirect` on the destination page],
)

`description` carries over under its own name, and fills the feed channel the
same way Zola's does.

== Frontmatter

The *language* needs no conversion: `+++` is TOML here too, and a block copies
over character for character (#link("../../write/markdown.typ")[Markdown pages]
has all three fences). TOML's date literal comes with it, so `date = 2026-07-09`
is read as the ISO day it spells, unquoted, exactly as Zola reads it.

What changes is the *names* of two keys, and where two of Zola's tables put their
contents:

```md
+++
title = "Hello"
date = 2026-07-09
order = 3                    # was weight
redirect = ["/old/hello/"]   # was aliases
tags = ["rust", "cli"]       # was under [taxonomies]
hero = "cover.png"           # was under [extra]
+++
```

#table(
  columns: 2,
  align: (left, left),
  table.header([Zola], [Baudelaire]),
  [`title`, `date`, `updated`, `draft`, `slug`], [the same names],
  [`description`], [the same name, and the same job: the feed blurb and the meta description.],
  [`weight`], [`order`],
  [`aliases`], [`redirect`],
  [`[taxonomies]`], [top-level keys, one per taxonomy declared in `config.kdl`.],
  [`[extra]`], [top-level keys, read as `page.frontmatter.<key>`.],
  [`template`], [the same name, naming a `.typ` file under `templates/`.],
  [`path`], [the same name, and the same meaning: the exact URL this page publishes at.],
  [`in_search_index`], [none],
)

A taxonomy has to be flat: `tags` is looked up at the top level and nowhere else,
so a `[taxonomies]` table left in place joins no term. `[extra]` is a choice - it
survives as a dict, so a template can read `page.frontmatter.extra.hero` instead
of flattening it. Flattening is what makes the key look like every other one.

#callout(kind: "warn")[
  *A key that changed name fails nothing.* Anything unrecognized passes through
  as extra frontmatter, so a post still carrying `weight`, `aliases` and a
  `[taxonomies]` table builds green with no ordering, no redirect and no terms.
  Only a near-miss of a real key (`titel`, `oder`) is an error. Grep for those
  three names before you trust the output.
]

A post you convert to `.typ` writes the same fields as a Typst dict instead:

```typ
#let frontmatter = (
  title: "Hello",
  date: "2026-07-09",
  order: 3,
  redirect: ("/old/hello/",),
  tags: ("rust", "cli"),
  hero: "cover.png",
)
```

There a list of one needs the trailing comma - `("rust")` is a string, `("rust",)`
is a list - which is the one thing TOML's `["rust"]` never made you think about.

== Templates

Tera inheritance becomes ordinary imports. A base template is a function you call
around your body, not a file you extend:

```html
<!-- templates/page.html -->
{% extends "base.html" %}
{% block content %}
  <h1>{{ page.title }}</h1>
  {{ page.content | safe }}
{% endblock %}
```

```typ
// templates/page.typ
#import "@baudelaire/html:0.1.0": h
#import "/templates/base.typ": shell

#let page(page, body) = shell(page)[
  #h("h1", page.frontmatter.title)
  #body
]
```

#table(
  columns: 2,
  align: (left, left),
  table.header([Tera], [Typst]),
  [`{{ page.title }}`], [`page.frontmatter.title`],
  [`{{ page.content | safe }}`], [`body`],
  [`{{ page.extra.hero }}`], [`page.frontmatter.at("hero", default: none)`],
  [`{{ page.taxonomies.tags }}`], [`page.taxonomies.at("tags", default: ())`],
  [`{% for p in section.pages %}`], [`for p in pages(page.lang).filter(p => p.collection == "blog")`],
  [`{% for p in paginator.pages %}`], [`for entry in page.frontmatter.entries` in a listing template],
  [`{% macro %}`], [`#let name(args) = ..`],
  [`{% include %}`], [`#import "/templates/parts.typ": ..`],
  [`{{ get_url(path="style.css") }}`], [`"/assets/style.css"`, rewritten if fingerprinting is on],
  [`{{ config.base_url }}`], [`url` from `@baudelaire/site`],
  [`{{ config.extra.x }}`], [`sys.inputs.baudelaire.client.x`, from a `client { }` constant],
  [`load_data(path="x.toml")`], [`toml("/data/x.toml")`, in the page or the template],
  [`resize_image()`], [`assets { images { responsive } }`],
  [`get_taxonomy()`], [a taxonomy listing template, or `page.taxonomies`],
  [`page.permalink`], [not available: a page does not know its own URL],
)

That last row is deliberate. Nothing site-wide may enter a page's wrapper, or
every page's cache identity would depend on every other page. Templates reach the
site through `@baudelaire/sections` and `@baudelaire/pages` instead, which are
tracked as files. See #link("../../write/templates.typ")[templates].

Template names are yours: there is no `page.html` / `section.html` /
`taxonomy_single.html` convention, only the `template` you bind on a collection,
a taxonomy, or a page.

== Shortcodes

Nothing interprets `{{ .. }}` or `{% .. %}` in a page. In markdown they are
prose, so a shortcode you forget ships to the reader as the braces you wrote and
no build fails. Grep the tree for them before you call the migration done.

A shortcode becomes a function in a file you import:

```typ
// templates/parts.typ
#let youtube(id) = html.elem("iframe", attrs: (
  src: "https://www.youtube-nocookie.com/embed/" + id,
  loading: "lazy",
  allowfullscreen: "true",
))
```

A markdown page calls it from a fence marked `eval`, which is the one place a
`.md` page runs Typst:

````md
```typ eval
#import "/templates/parts.typ": youtube
#youtube("dQw4w9WgXcQ")
```
````

A converted page calls it as `#youtube("dQw4w9WgXcQ")` in the body. Body
shortcodes (`{% quote() %} .. {% end %}`) take the body as a content argument:
`#let quote(body) = ..`, called as `#quote[..]`. Most shortcodes only wrapped a
tag or two and are not worth a function at all: write the markup.

== Internal links

A link names the *source file*, `.md` or `.typ`, and the build rewrites it to
that page's permalink. `@/blog/hello.md` is Zola's spelling of the same idea and
means nothing here - but it no longer slips through either, since `@` is not a
directory and the target does not resolve:

```text
× `@/blog/other.md` has no matching page
  ╭─[blog/hello.md:7:16]
7 │ A post. [zola](@/blog/other.md)
  ·                ───────┬───────
  ·                       ╰── no page here
```

#table(
  columns: 2,
  align: (left, left),
  table.header([Written], [What the build does]),
  [`[a](hello.md)`, `[a](../notes/hello.md)`], [Rewritten to that page's permalink, resolved against the linking file.],
  [`[a](/content/blog/hello.md)`], [The same, resolved against the project root. This is what `@/blog/hello.md` becomes.],
  [`[a](hello.typ)`, `#link("hello.typ")`], [The same again, for a page you converted.],
  [`@/blog/hello.md`], [Fails the build, naming the line.],
  [`[a](/blog/hello/)`], [Passed through as authored, and not checked.],
)

So the `@/` links are a mechanical fix the build finds for you, one page at a
time. Old absolute URLs are the ones nothing checks: they survive a copy and
break on the first rename, which #link("urls.typ")[keeping your URLs] is about.
See #link("../../write/pages.typ")[pages].

== Raw HTML

Zola passes raw HTML in a post straight through. Here it fails the build, on the
line it sits on: the DOM this build produces is typed, and there is nowhere to
splice a string of markup into it. Three answers:

#table(
  columns: 2,
  align: (left, left),
  table.header([Do], [Get]),
  [`content { markdown { html "drop" } }`], [An inline run loses its tags and keeps its prose; a *block* loses its contents with them.],
  [a `typ eval` fence], [`html.elem("aside", ..)`, the same element written in the typed form.],
  [convert the page], [`h("div", class: "x")[..]` from `@baudelaire/html`.],
)

An HTML comment is dropped either way: it has nothing to lose.

== What Zola wrote that this does not

#table(
  columns: 2,
  align: (left, left),
  table.header([Zola output], [Here]),
  [`/blog/page/1/`], [not written. Page 1 of an index is only its own URL; a top-level `redirect { "/blog/page/1/" "/blog/" }` claims the old path back.],
  [`404.html`], [authored: write `content/404.typ`. Zola ships a default template, so this one is easy to lose in the move.],
  [`/blog/hello/cover.png` beside the post], [`/assets/blog/hello/cover.png`. A colocated file is published under the asset tree, mirroring the directory it sat in, not beside the page.],
  [`/style.css` from `sass/`], [`/assets/style.css`. The asset tree's last path segment is the URL prefix, and only `static/` publishes at the root.],
  [`search_index.en.js` + `elasticlunr.min.js`], [`search.json`, and `generate { search { ui } }` for a bundled palette. Custom search code is rewritten, not ported.],
  [a feed per term], [the same, under `generate { feed { terms #true } }`.],
)

== Commands

#table(
  columns: 2,
  align: (left, left),
  table.header([Zola], [Baudelaire]),
  [`zola build`], [`baudelaire build`],
  [`zola serve`], [`baudelaire serve`],
  [`zola check`], [`baudelaire check`],
  [`zola init`], [`baudelaire init`],
)

== Order of work

+ `baudelaire init` beside the old site, then copy `static/` across and keep `sass/` where it is, compiled by a `hooks { before "sass sass/style.scss assets/style.css" }` line. Keep the `.scss` sources out of `paths { assets }`, or they are published beside the CSS.
+ Copy `content/` across as it stands, `+++` blocks included, and delete the `_index.md` files.
+ Rename the frontmatter keys that moved: `weight` to `order`, `aliases` to `redirect`, and lift `[taxonomies]` to the top level. Nothing fails if you miss one, so this is a grep over the tree, not a build.
+ Port `config.toml` to `config.kdl`, one block at a time. An unknown key fails the build with a suggestion, so this converges fast.
+ Rewrite the templates one section needs. Everything else can stay unstyled while you work.
+ Strip the `@/` from the internal links - the build names them - then replace the shortcodes and convert the pages that actually want Typst.
+ `baudelaire check` until it is quiet, then compare sitemaps as in #link("urls.typ")[keeping your URLs].
