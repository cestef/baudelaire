#let frontmatter = (
  title: "From Zola",
  order: 5,
)
#import "/templates/theme.typ": callout

The closest neighbour: one Rust binary, a content tree, a template directory,
taxonomies and pagination in the config. Most of the move is mechanical. The two
real jobs are converting Markdown to Typst and rewriting Tera templates as Typst
functions.

== The tree

#table(
  columns: 3,
  align: (left, left, left),
  table.header([Zola], [Baudelaire], [Note]),
  [`config.toml`], [`config.kdl`], [KDL, and every key is checked.],
  [`content/**/*.md`], [`content/**/*.typ`], [See #link("content.typ")[Markdown to Typst].],
  [`content/blog/_index.md`], [a `blog` block in `config.kdl`], [There is no per-section file.],
  [`templates/*.html`], [`templates/*.typ`], [Tera becomes Typst functions.],
  [`templates/shortcodes/`], [any `.typ` file you import], [No registry.],
  [`sass/`], [`assets/`], [No Sass step; see below.],
  [`static/`], [`static/`], [Copied verbatim, same as Zola.],
  [`themes/blow`], [`theme "themes/blow"`], [Different format; a theme is not portable.],
  [`public/`], [`dist/`, renamed with `paths { dist }`], [],
)

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
  [`[link_checker]`], [`links { external #true }`],
  [`[slugify]`], [fixed rules; override per page with `slug`],
  [`[extra]`], [`client { }` for the browser, `typst { inputs }` for the compiler],
)

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

```text
+++
title = "Hello"
date = 2026-07-09
weight = 3
aliases = ["/old/hello/"]
[taxonomies]
tags = ["rust"]
[extra]
hero = "cover.png"
+++
```

becomes

```typ
#let frontmatter = (
  title: "Hello",
  date: datetime(year: 2026, month: 7, day: 9),
  order: 3,
  redirect: ("/old/hello/",),
  tags: ("rust",),
  hero: "cover.png",
)
```

#table(
  columns: 2,
  align: (left, left),
  table.header([Zola], [Baudelaire]),
  [`title`, `description`, `date`, `updated`, `draft`, `slug`, `template`], [the same names],
  [`weight`], [`order`],
  [`aliases`], [`redirect`],
  [`[taxonomies]` tables], [top-level lists, one per declared taxonomy],
  [`[extra]` tables], [top-level keys, read as `page.frontmatter.<key>`],
  [`path`], [the same name, and the same meaning: the exact URL this page publishes at.],
  [`in_search_index`], [none],
)

Taxonomy terms and extra values are flat here, not nested, so `[taxonomies]
tags` and `[extra] hero` both land at the top level. A key that is neither
recognized nor a taxonomy passes through as extra, and one that merely looks like
a typo of a recognized key is an error.

Zola's frontmatter is TOML, which pandoc does not read as metadata, so strip the
`+++` block before converting a post. The one-liner is on
#link("content.typ")[Markdown to Typst], along with what else the conversion
leaves behind.

#callout(kind: "warn")[
  Single-element lists need the trailing comma. `("rust")` is a string;
  `("rust",)` is a list. This is the most common first-day mistake.
]

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

A Zola shortcode is a template in a magic directory. Here it is a function in a
file you import:

```typ
// templates/parts.typ
#let youtube(id) = html.elem("iframe", attrs: (
  src: "https://www.youtube-nocookie.com/embed/" + id,
  loading: "lazy",
  allowfullscreen: "true",
))
```

```typ
#import "/templates/parts.typ": youtube
#youtube("dQw4w9WgXcQ")
```

Body shortcodes (`{% quote() %} .. {% end %}`) take the body as a content
argument: `#let quote(body) = ..`, called as `#quote[..]`.

A `@/blog/hello.md` link is Zola's internal-link syntax and means nothing here.
It becomes `#link("hello.typ")`, a path relative to the linking file, which the
build resolves and checks.

== What Zola wrote that this does not

#table(
  columns: 2,
  align: (left, left),
  table.header([Zola output], [Here]),
  [`/blog/page/1/`], [not written. Page 1 of an index is only its own URL; a top-level `redirect { "/blog/page/1/" "/blog/" }` claims the old path back.],
  [`404.html`], [authored: write `content/404.typ`. Zola ships a default template, so this one is easy to lose in the move.],
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
+ Port `config.toml` to `config.kdl`, one block at a time. An unknown key fails the build with a suggestion, so this converges fast.
+ Convert one section's Markdown, with #link("content.typ")[pandoc].
+ Rewrite the templates that section needs. Everything else can stay unstyled while you work.
+ `baudelaire check` until it is quiet, then compare sitemaps as in #link("urls.typ")[keeping your URLs].
