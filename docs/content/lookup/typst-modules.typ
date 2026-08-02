#let frontmatter = (
  title: "Typst modules",
  order: 2,
)
#import "/templates/theme.typ": callout

Four packages your templates can import without anything existing on disk.
Nothing is downloaded: typst asks for the package, baudelaire answers it.

```typ
#import "@baudelaire/html:0.1.0": h, classes
#import "@baudelaire/site:0.1.0": title, url

#h("a", class: "brand", href: "/")[#title]
```

#table(
  columns: 3,
  align: (left, left, left),
  table.header([Module], [Exports], [Gives you]),
  [`@baudelaire/html`], [`h`, `classes`, `svg`], [Element construction and SVG inlining.],
  [`@baudelaire/site`], [`version`, `title`, `url`, `lang`, `author`, `languages`], [Site identity as typed bindings.],
  [`@baudelaire/sections`], [`sections(lang)`], [The site's content tree.],
  [`@baudelaire/pages`], [`pages(lang)`], [Every authored page as a row.],
)

They are the Typst counterpart of the #link("js-modules.typ")[`baudelaire:*`
JavaScript modules] and read from the same build data, so a template and a bundle
can never disagree about what the site is called.

== Versions

*Every module is served at `0.1.0`, and that is the only version there is.* Write
`:0.1.0` on every import and stop thinking about it. Typst's package syntax
requires a version, and this one tracks the module API, not baudelaire's own
version, so it does not move when you upgrade. Asking for any other version fails
at the import, naming the one that exists, rather than reaching for the network.

== Editor support

These packages only exist while baudelaire is compiling, so an editor has
nothing to resolve and marks the import unknown. Write them out once:

```sh
baudelaire packages
```

That installs all four into typst's own package directory, where tinymist and
anything else built on typst finds them. `baudelaire init` does it for you.

#table(
  columns: 2,
  align: (left, left),
  table.header([Flag], [Does]),
  [`--path DIR`], [Install somewhere else. Point your editor at it with `tinymist.packagePath`.],
)

A build never reads what this writes: the compiler answers `@baudelaire/*` from
memory before typst's package resolution runs, so an installed copy that is
stale, edited or missing can mislead an editor and never change a page.

#callout(kind: "note")[
  `sections` and `pages` are built from your own pages, so they install with the
  data from the last build, and empty in a project that has never been built.
  The symbols resolve either way. Re-run after upgrading baudelaire.
]

== html

`h` is `html.elem` without the `attrs:` wrapper. Named arguments become
attributes, positional ones become children.

```typ
// before
#html.elem("button", attrs: (class: "icon-btn", type: "button"), body)
// after
#h("button", class: "icon-btn", type: "button", body)
```

Hyphenated names need no quoting in either form, since a Typst identifier may
contain a hyphen: write `aria-label: "Close"`, not `"aria-label": "Close"`.

Attribute values follow what HTML wants, which removes most of the `if`s and
`str()` calls a template accumulates:

#table(
  columns: 2,
  align: (left, left),
  table.header([Value], [Writes]),
  [`true`], [A bare boolean attribute, so `data-open: true` writes `data-open`.],
  [`none` or `false`], [Nothing. `h("a", href: target)` is safe when `target` is missing.],
  [anything else], [The coerced string, so `width: size` needs no `str(size)`.],
)

A computed attribute dict spreads in, which is how you build an element whose
attributes are data:

```typ
#for (tag, attrs) in shapes { h(tag, ..attrs) }
```

`classes` joins class names, skipping what is absent and taking a
`(name, condition)` pair for a conditional one:

```typ
#h("div", class: classes("callout", "callout-" + kind, ("active", current)))
```

#callout(kind: "note")[
  `"a" + if cond { " b" }` looks like it should work, but the else branch is
  `none` and adding it to a string fails the build. An empty `classes` result is
  `none`, which `h` then omits, so you never get a stray `class=""`.
]

=== svg

`svg()` puts an SVG file's own markup into the page as real DOM:

```typ
#svg("/icons/search.svg", class: "icon", aria-hidden: "true")
```

Not the same as `image("/icons/search.svg")`, which produces an `<img>`. An
`<img>` is opaque: CSS cannot reach inside it, so an icon drawn that way cannot
inherit `currentColor`, cannot be recolored by a theme toggle, and cannot carry
your own class or ARIA attributes. Use `image()` for photographs, `svg()` for
icons.

The file's own root attributes fill in under yours, so one file serves every call
site and you override only what differs:

```typ
#svg("/icons/search.svg", width: 16, height: 16)  // its viewBox, your size
```

The path is from the project root and starts with `/`. It cannot be relative to
the calling template: baudelaire reads the file after the compile, not typst, so
there is no template to resolve it against. Any project file works, so icons can
live outside `assets/` and never be published on their own.

Comments, XML declarations, DOCTYPEs, and an editor's private namespaces
(Inkscape's `sodipodi:*`, `dc:title` inside `<metadata>`) are dropped on the way
in. `xlink:href` becomes plain `href`, the SVG 2 spelling.

#callout(kind: "warn")[
  A file carrying a `<script>`, an `on*` handler, or a `javascript:` URL is
  refused. Inlining makes it part of the page, so it would run with your origin.
]

A `<style>` inside the file is kept, because Illustrator exports rely on one. Its
rules are confined to the icon, which is why an icon with a stylesheet gains a
`data-svg` attribute:

```css
/* authored */   .st0{fill:#231F20}
/* emitted  */   :where([data-svg="051c4bdc"]) .st0{fill:#231F20}
```

`:where()` adds no specificity, so a confined rule loses to your page CSS exactly
as it did before. `@media`, `@supports`, `@container` and `@layer` are descended
into; `@keyframes` and `@font-face` name their own thing, so they are left alone.
A selector targeting the icon's own root (`svg { .. }` inside the file) will not
match, since the rules are rewritten as descendants. Style the shapes instead.

== site

Site identity as plain bindings, rather than a chain of guarded `.at` reads into
`sys.inputs`:

```typ
#import "@baudelaire/site:0.1.0": version, title, url, lang, author, languages
```

#table(
  columns: 3,
  align: (left, left, left),
  table.header([Binding], [Type], [Is]),
  [`version`], [str], [Baudelaire's version.],
  [`title`], [str or none], [`site` from the config.],
  [`url`], [str or none], [The canonical base URL.],
  [`lang`], [str], [The default language code.],
  [`author`], [str or none], [`author` from the config.],
  [`languages`], [array], [`(code, name)` dicts, default first. Empty unless #link("../write/i18n.typ")[i18n] is on.],
)

Every name is always bound, and unset config reads as `none`, so a theme can ask
for `author` on a site that never set one. A name that does not exist fails at
the import instead of quietly reading back `none`.

#callout(kind: "note")[
  Build metadata that changes between builds is deliberately not here. Read `git`
  and `date` from #link("context.typ")[`sys.inputs.baudelaire`], where baudelaire
  tracks which page read which value. A copy baked into a module would rebuild
  the whole site on every commit.
]

== sections

The site's own view of `content/`, as a tree, for a nav that cannot drift from
the pages.

```typ
#import "@baudelaire/sections:0.1.0": sections

#let layout(page, body) = {
  for section in sections(page.lang) {
    for entry in section.pages { link(entry.url)[#entry.title] }
  }
  body
}
```

`sections(lang)` takes a language code (pass `page.lang`, which is correct on a
single-language site too) and returns that language's tree, or an empty array for
a language with no pages. Each node is:

#table(
  columns: 2,
  align: (left, left),
  table.header([Field], [Is]),
  [`id`], [The directory name, one segment.],
  [`pages`], [`(url, title)` dicts for the pages directly in it.],
  [`children`], [Nested nodes for its subdirectories.],
)

Generated pages and the not-found page are excluded. See
#link("../write/collections/navigation.typ")[navigation] for the ordering rules.

== pages

Every authored page of one language, as rows a template can filter and render: a
home page showing the three most recent posts, a portfolio grid, a related list,
an archive.

```typ
#import "@baudelaire/pages:0.1.0": pages

#let recent(page) = {
  let posts = pages(page.lang).filter(p => p.collection == "posts")
  for entry in posts.slice(0, calc.min(3, posts.len())) {
    link(entry.url)[#entry.label]
  }
}
```

A row is the same shape a generated listing hands its template as
`page.frontmatter.entries`:

#table(
  columns: 2,
  align: (left, left),
  table.header([Field], [Is]),
  [`url`], [The page's permalink.],
  [`label`], [Its title.],
  [`collection`], [The collection it belongs to.],
  [`lang`], [Its language code.],
  [`date`], [The date, ISO, or `none`.],
  [`display`], [The same date, localized, or `none`.],
  [`note`], [A trailing annotation, or `none`.],
  [`taxonomies`], [A dict of `taxonomy -> (terms..)`.],
  [`extra`], [The page's own remaining frontmatter.],
)

The shape is shared on purpose: the card component a theme writes for its
collection index renders a home-page grid unchanged. Generated listings are not
in the catalogue, and neither is the not-found page. Pages come in the site's own
order, collection by collection, each in its collection's sort order.

#callout(kind: "note")[
  `sections` and `pages` are written to a file under `.baudelaire/` and served
  from there. Their content depends on every page in the site, so serving them
  from memory would make one rename recompile everything. As files, typst records
  an ordinary file dependency and only the templates that import them rebuild.
]
