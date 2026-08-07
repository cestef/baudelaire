#let frontmatter = (
  title: "Templates",
  order: 4,
)
#import "/templates/theme.typ": callout

A template is a Typst file in `templates/` exporting one function, named after the file, taking `(page, body)`.

```typ
// templates/page.typ
#import "@baudelaire/html:0.1.0": h

#let page(page, body) = h("main", {
  h("h1", page.frontmatter.title)
  body
})
```

`body` is the compiled page. `page` is everything the build knows about it. The function name has to match the file stem: `page.typ` exports `page`, `post-card.typ` exports `post-card`.

A page with no template bound anywhere is written out as Typst renders it, with no wrapper at all.

== Bind it

Per collection, in `config.kdl`, which is how most pages get one. `_root` is the collection a page directly under `content/` lands in, so binding it there covers the home page and its neighbours:

```kdl
content {
  collections {
    _root { template "page.typ" }
    posts "posts/**/*.typ" {
      template "page.typ"
      paginate {
        template "list.typ"
        size 10
      }
    }
  }
}
```

Per page, in frontmatter, which wins over both:

```typ
#let frontmatter = (title: "Home", template: "home.typ")
```

The nearer binding wins: a page's own frontmatter, else its collection's. A #link("../start/themes.typ")[theme] states the collection half in its own `theme.kdl`, which is what makes a themed page render without naming anything.

Paths are file names inside `paths { templates }` (`templates/` by default). #link("collections/taxonomies.typ")[Taxonomy] listings take a `template` of their own, and so do #link("../build/generate/cards.typ")[social cards] and #link("../build/generate/pdf.typ")[PDFs], though those two are paged documents rather than HTML.

== What `page` carries

#table(
  columns: 3,
  align: (left, left, left),
  table.header([Field], [Type], [Holds]),
  [`frontmatter`], [dict], [The page's own `#let frontmatter`, exactly as authored.],
  [`taxonomies`], [dict], [Parsed terms per configured taxonomy, e.g. `(tags: ("rust",))`.],
  [`nav`], [dict], [`(prev, next)`, each `none` or `(url, title)`.],
  [`lang`], [str], [The page's language code, `"en"` on a single-language site.],
  [`translations`], [array], [`(lang, url, title)` per edition, this page's included.],
  [`strings`], [dict], [The current language's UI string table.],
  [`reading`], [dict], [`(words, minutes)`, from the page's own source.],
  [`backlinks`], [array], [`(url, title, lang, fragments)` per page whose content links here. Empty unless `links { backlinks }` is on.],
  [`date`], [dict], [`(iso, display)`, or `none` when the page carries no date.],
  [`url`], [str], [Where this page publishes, base path included.],
  [`collection`], [str], [The collection it belongs to.],
  [`assets`], [dict], [The files beside it in its own bundle, authored name to served URL. Empty unless the page is a bundle.],
)

Read optional frontmatter defensively, since a page may not declare it:

```typ
#let summary = page.frontmatter.at("summary", default: none)
#let tags = page.taxonomies.at("tags", default: ())
```

#callout(kind: "note")[
  What a page knows is *itself*: its own URL, its own collection, its own directory. Nothing in the wrapper may name _other_ pages, or every page's cache identity would depend on all of them. The site tree and the page catalogue come from #link("../lookup/typst-modules.typ")[virtual modules] instead: `sections(page.lang)` and `pages(page.lang)`.
]

`page.nav` and `page.backlinks` sit right on that line, and only because each names a page's neighbours rather than the site. See #link("collections/navigation.typ")[prev / next links], #link("backlinks.typ")[backlinks], #link("reading.typ")[reading time], and #link("i18n.typ")[multiple languages] for the fields that have a page of their own.

== A bundle's own files

A #link("pages.typ")[page bundle] (`content/posts/hello/index.typ`) owns its directory, so `page.assets` maps each file there to the URL it is served from:

```typ
#let frontmatter = (title: "Hello", hero: "cover.png")
```

```typ
#let page(page, body) = {
  let hero = page.frontmatter.at("hero", default: none)
  if hero != none { h("img", src: page.assets.at(hero), alt: "") }
  body
}
```

A page that shares its directory with its neighbours has no directory of its own, and its `assets` is empty: a listing of the whole section would be a site-wide value in disguise. Only a file the page actually shows is written to `dist`, so name it in the body as well (`#image("cover.png")`) or it has a URL and no file. See #link("../build/images.typ")[images].

== Listing templates

A generated index (a paginated collection, a taxonomy term, the term index) has no source file, so its `page.frontmatter` is built for it:

```typ
// templates/list.typ
#import "@baudelaire/html:0.1.0": h

#let list(page, body) = h("main", {
  h("h1", page.frontmatter.title)
  h("ul", for entry in page.frontmatter.entries {
    h("li", h("a", href: entry.url, entry.label))
  })
})
```

Each row:

#table(
  columns: 3,
  align: (left, left, left),
  table.header([Field], [Type], [Holds]),
  [`url`], [str], [Where the row points.],
  [`label`], [str], [The source page's title.],
  [`collection`], [str], [The collection it belongs to.],
  [`lang`], [str], [Its language code.],
  [`date`], [str], [ISO-8601 day, or `none`.],
  [`display`], [str], [The same date written the way its language writes one, or `none`.],
  [`note`], [str], [A trailing annotation (a term index puts its count here), or `none`.],
  [`description`], [str], [Its one-line summary, from `description` or the `summary` alias, or `none`.],
  [`image`], [str], [Its own social image, or `none`.],
  [`alt`], [str], [What that image shows, or `none`. Empty marks it decorative.],
  [`author`], [str], [Who wrote it, or `none`. The page's own; never the site default.],
  [`taxonomies`], [dict], [The source page's terms.],
  [`extra`], [dict], [Frontmatter baudelaire does not name: the theme's own keys.],
)

`page.frontmatter.nav` on a listing is pagination: `prev` and `next` are plain URL strings, not the `(url, title)` dicts `page.nav` uses on a real page. `body` is a generated fallback list, which a template that draws `entries` itself can ignore.

The row shape is shared with `@baudelaire/pages`, so one card component renders a collection index, a term page, and a home-page grid unchanged.

== `h()`

`html.elem` is the honest way to emit an element and it is wordy. `h` is the same thing with the `attrs:` wrapper removed.

```typ
#import "@baudelaire/html:0.1.0": h, classes, svg

// before
#html.elem("button", attrs: (class: "icon-btn", type: "button"), body)
// after
#h("button", class: "icon-btn", type: "button", body)
```

Named arguments become attributes, positional ones become children. Hyphenated names need no quoting, since a Typst identifier may contain a hyphen: `aria-label: "Close"`.

#callout(kind: "tip")[
  These packages exist only while baudelaire compiles, so an editor marks the
  import unresolved. `baudelaire mirror` writes them out where tinymist looks.
  See #link("../lookup/typst-modules.typ")[typst modules].
]

Values follow what HTML actually wants, which removes most of the guards a template accumulates:

#table(
  columns: 2,
  align: (left, left),
  table.header([Value], [Emits]),
  [`true`], [A bare boolean attribute: `data-open: true` writes `data-open`.],
  [`none` or `false`], [Nothing. `h("a", href: target)` is safe when `target` is missing.],
  [anything else], [The coerced value, so `width: size` needs no `str(size)`.],
)

`classes` joins class names, skipping what is absent and taking a `(name, condition)` pair for a conditional one. `svg` inlines an icon file as real DOM. Both are covered on #link("../lookup/typst-modules.typ")[Typst modules], along with every other `@baudelaire/*` package.

#callout(kind: "warn")[
  A template must never emit `html`, `head`, or `body` elements. typst-html owns those. A page whose layout emits a single `html` root hands Typst the author's document instead, and everything that appends to the head, meta tags and verification links included, silently disappears.
]

== Sharing pieces

Templates import each other like any Typst file, so a shell, a byline, and a card row live in one file that every layout pulls from:

```typ
#import "/templates/parts.typ": byline, shell
```

That is a project-root path, which is right for a site. A #link("theme-authoring.typ")[theme] uses relative imports instead, and keeps its shared file outside `templates/`.
