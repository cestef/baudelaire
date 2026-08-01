#let frontmatter = (
  order: 8,
  title: "Themes",
  tags: ("guide", "themes"),
)
#import "/templates/theme.typ": callout, link-to

A theme is a site's look, packaged: templates, assets, and config defaults in
one directory. Name one and your pages have a design; override any file of it by
having your own at the same path. This chapter goes from picking one to
publishing your own.

== Pick one

Four ship with baudelaire, one per kind of site. Pick by what the site *is*,
not by what it looks like: the look is the part you can change in an afternoon.

/ #link("/themes/albatros/")[albatros]: you write posts. A centred column,
  tags, reading time, and a language switcher if the site has editions.
/ #link("/themes/spleen/")[spleen]: you write posts and want no JavaScript at
  all. A terminal, dark first.
/ #link("/themes/phares/")[phares]: you document something. A sidebar built from
  your own `content/` tree, a search palette, and the page's headings down the
  right.
/ #link("/themes/paysage/")[paysage]: you show work. A landing page, a grid of
  projects, and one case study per project.

The #link("/themes/")[themes page] has a live demo of each.

== Adopt it

Copy the theme's directory into your project and name it:

```kdl
theme "themes/albatros"
```

The directory has to sit inside your project, because a Typst import cannot
reach outside the project root. Installed into your Typst package directory
instead, it is named as a package and shared across projects:

```bash
cp -r themes/albatros ~/.local/share/typst/packages/local/albatros/0.1.0
```

```kdl
theme "@local/albatros:0.1.0"
```

Only the `preview` namespace is ever downloaded, so `@local` and any other
namespace are read from disk and never hit the network.

Then check the theme's README for what it expects from a page. All four want a
`title`; most use `date`, `summary`, and a taxonomy, and each documents the rest
in a table.

#callout(kind: "note")[
  A theme's `theme.kdl` is a *floor*. Every key you state wins, nested blocks
  included, so adopting one never touches your `site`, `url`, or `author`.

  Lists are the exception: a `content { collections { .. } }` of your own
  replaces the theme's set wholesale rather than adding to it. If you only meant
  to add a collection, copy the theme's block first.
]

== Change part of it

The rule is one line: *your file wins*, file by file.

/ A layout: copy `templates/page.typ` out of the theme into your own
  `templates/` and edit it. Every other template still comes from the theme.
/ The look: ship your own `assets/style.css` and it replaces the theme's. Every
  other asset still comes from the theme.
/ A colour or a width: every theme here declares its palette as custom
  properties at the top of `style.css`. Restating a few of them in a stylesheet
  of your own is smaller than forking the file.

```css
/* assets/tweaks.css, loaded after the theme's */
:root {
  --accent: #7a3ea1;
  --measure: 48rem;
}
```

== Write one

Start by copying the closest theme. The directory layout is fixed, because a
theme cannot know what you renamed your own directories to:

```text
templates/   layouts a page binds to, by filename
assets/      processed by the asset pipeline
static/      copied verbatim
theme.kdl    config defaults
typst.toml   package manifest, for publishing
lib.typ      what a page can import from the theme
```

A layout is an ordinary template: a function named after its file, taking the
page and its body.

```typ
#import "@baudelaire/html:0.1.0": h
#import "../parts.typ": shell

#let page(page, body) = shell(page, h("article", {
  h("h1", page.frontmatter.title)
  body
}))
```

Three rules bite theme authors and nobody else:

- *Import siblings relatively.* Inside a theme, `#import "../parts.typ"`
  resolves both when the theme is a directory and when it is an installed
  package. A root-absolute `/parts.typ` resolves against the *project* root in
  the first case and the *package* root in the second, so it cannot be right in
  both.
- *`svg()` is off limits.* Its paths are project-root absolute, and a theme does
  not know where it sits in your project. Build icons as inline `<svg>` elements
  instead, which is what `parts.typ` does in every theme here.
- *Keep shared pieces out of `templates/`.* Only `templates/`, `assets/` and
  `static/` are layered, so a `templates/parts.typ` can be shadowed by a project
  file that never meant to. At the theme root it cannot.

=== Build the nav from the site, not from config

None of the shipped themes hardcode a menu, and yours should not either. Two
#link("../features/build/typst-modules.typ")[virtual modules] hand you the
build's own view of the site:

```typ
#import "@baudelaire/sections:0.1.0": sections
#import "@baudelaire/pages:0.1.0": pages

// every content directory that holds pages, nested
#let directories = sections(page.lang)
// every authored page of this language, as listing rows
#let posts = pages(page.lang).filter(p => p.collection == "posts")
```

A page catalogue row is the same shape a generated listing hands its template as
`entries`, so one card component renders a collection index, a term page, and a
home-page grid. That is how `albatros` shows recent posts and `paysage` builds
its work grid without either of them listing anything by hand.

=== Every visible word

Read UI text from the site's own string table, so a non-English site translates
your theme through config rather than by editing it:

```typ
#let label(page, key, fallback) = page.strings.at(key, default: fallback)

#label(page, "reading", "min read")
```

Dates arrive already localized: `page.date.display` is written the way the
page's language writes one, and `page.date.iso` is what a `<time datetime>`
wants. Never format one yourself, or a French page ends up reading `July`.

=== Publish it

`typst.toml` is an ordinary Typst package manifest. Once published, the theme is
named like any other package, and the whole change on a site using it is one
line:

```kdl
theme "@preview/plume:0.1.0"
```

#callout(kind: "warning")[
  A template must never emit `<html>`, `<head>` or `<body>`: typst-html owns
  them. A page whose layout emits a single `<html>` element hands typst the
  author's root instead, and everything that appends to the head, meta tags and
  verification links included, silently disappears.
]
