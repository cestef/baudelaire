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

Most pages get a template from their collection, set in `config.kdl`. `_root` is the collection a page directly under `content/` lands in, so binding it there covers the home page and its neighbours:

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

A page can also set one in its frontmatter, which wins:

```typ
#let frontmatter = (title: "Home", template: "home.typ")
```

A page's own frontmatter wins, else its collection's. A #link("../theming/using.typ")[theme] sets the collection template in its own `theme.kdl`, so a themed page renders without naming one.

Paths are file names inside `paths { templates }` (`templates/` by default). #link("../collections/taxonomies.typ")[Taxonomy] listings take a `template` of their own, and so do #link("../build/generate/cards.typ")[social cards] and #link("../build/generate/pdf.typ")[PDFs], though those two are paged documents rather than HTML.

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
  [`source`], [str or none], [The page's own file, project-root-absolute (`/content/posts/hello.typ`). `none` on a generated listing, which has no file.],
  [`git`], [dict or none], [What the repository's log says about that file. `none` unless `content { history }` is on.],
)

`page.source` is what an "edit this page" link is built from: it is
root-absolute, so you can append it to a forge URL directly.

```typ
#let edit = "https://github.com/you/site/edit/main" + page.source
```

`page.git` holds when the page last changed and who changed it. It is off by
default, because reading it walks the repository's log:

```kdl
content {
  history {
    contributors #true    // gather every author, not just the last one
  }
}
```

#table(
  columns: 3,
  align: (left, left, left),
  table.header([Key], [Type], [Is]),
  [`hash`], [str], [The commit that last touched the file.],
  [`committed`], [str], [Its date, ISO 8601.],
  [`author`], [dict], [`(name, email)` of whoever made it.],
  [`contributors`], [array], [`(name, email, commits)` per author, most commits first. Absent unless `contributors` asked for them.],
)

```typ
#let git = page.at("git", default: none)
#if git != none [
  Last updated #git.committed.slice(0, 10) by #git.author.name.
]
```

`none` for a page no commit has touched yet, for a generated listing, and for a
build outside a repository. Read it defensively: a page written since the last
commit has no history, which is not an error.

This is also where the sitemap's `lastmod` comes from. Frontmatter `updated`
wins where a page declares it; otherwise the commit date is used, and the publication `date` only where neither
is available.

Read optional frontmatter defensively, since a page may not declare it:

```typ
#let summary = page.frontmatter.at("summary", default: none)
#let tags = page.taxonomies.at("tags", default: ())
```

#callout(kind: "note")[
  `page` knows only about itself: its own URL, collection and directory. It may not name other pages, or every page's cache entry would depend on all of them. The site tree and the page catalogue come from #link("../lookup/typst-modules.typ")[virtual modules] instead: `sections(page.lang)` and `pages(page.lang)`.
]

`page.nav` and `page.backlinks` are the near exceptions: each names a page's neighbours, not the whole site. See #link("../collections/navigation.typ")[prev / next links], #link("backlinks.typ")[backlinks], #link("reading.typ")[reading time], and #link("i18n.typ")[multiple languages] for the fields that have a page of their own.

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

`html.elem` emits an element but is wordy. `h` is the same thing without the `attrs:` wrapper.

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

Values are coerced the way HTML expects, so a template needs fewer guards:

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
  A template must never emit `html`, `head`, or `body` elements. typst-html owns those. If a layout emits a single `html` root, Typst uses it as the document, and everything that appends to the head (meta tags, verification links) silently disappears.
]

== Sharing pieces

Templates import each other like any Typst file, so a shared shell, byline or card row can live in one file:

```typ
#import "/templates/parts.typ": byline, shell
```

That is a project-root path, which suits a site. A #link("../theming/authoring.typ")[theme] uses relative imports instead, and keeps its shared file outside `templates/`.
