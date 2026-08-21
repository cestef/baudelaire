#let frontmatter = (
  title: "Typst modules",
  order: 2,
)
#import "/templates/theme.typ": callout

Seven packages your templates and pages can import without anything existing on
disk. Nothing is downloaded: typst asks for the package, baudelaire answers it.

```typ
#import "@baudelaire/html:0.1.0": h, classes
#import "@baudelaire/site:0.1.0": title, url

#h("a", class: "brand", href: "/")[#title]
```

#table(
  columns: 3,
  align: (left, left, left),
  table.header([Module], [Exports], [Gives you]),
  [`@baudelaire/html`], [`h`, `classes`, `svg`, `nolint`], [Element construction, SVG inlining, and keeping the #link("../build/linting.typ")[lint] off a region.],
  [`@baudelaire/site`], [`version`, `title`, `url`, `lang`, `author`, `languages`, `feeds`], [Site identity as typed bindings.],
  [`@baudelaire/sections`], [`sections(lang)`], [The site's content tree.],
  [`@baudelaire/pages`], [`pages(lang)`], [Every authored page as a row.],
  [`@baudelaire/markdown`], [`md`], [Markdown rendered inside a Typst page.],
  [`@baudelaire/sources`], [one per declaration], [The files `paths { sources }` declared.],
  [`@baudelaire/toc`], [`toc`], [A table of contents, from the page's own headings.],
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
baudelaire mirror
```

That writes them all into `.baudelaire/generated/packages/`, alongside the
#link("js-modules.typ")[`baudelaire:*` declarations]. `baudelaire init` does it
for you.

They go in the project rather than in typst's own package directory because
three of them describe *this* site, and one machine-global copy would show one
project's title and pages to every other project's editor. So point typst at
the directory once:

```sh
export TYPST_PACKAGE_PATH="$PWD/.baudelaire/generated/packages"
```

```json
//! @ignore
// tinymist
"tinymist.typstExtraArgs": ["--package-path", "/abs/path/.baudelaire/generated/packages"]
```

#table(
  columns: 2,
  align: (left, left),
  table.header([Flag], [Does]),
  [`--global`], [Write into typst's own package directory instead: nothing to configure, one copy shared by every project.],
  [`--path DIR`], [Write somewhere else again.],
)

A build never reads what this writes: the compiler answers `@baudelaire/*` from
memory before typst's package resolution runs, so an installed copy that is
stale, edited or missing can mislead an editor and never change a page.

#callout(kind: "note")[
  `sections` and `pages` are built from your own pages, so they install with the
  data from the last build, and empty in a project that has never been built.
  The symbols resolve either way. Re-run after upgrading baudelaire.
]

`baudelaire mirror --uninstall` takes them back off, removing baudelaire's own
namespace directory and nothing else in there, and the declarations with it.
`clean` sweeps them too, since they are project state now; a `--global` install
is not project state, so only `--uninstall --global` takes that one off.

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
#import "@baudelaire/site:0.1.0": version, title, url, lang, author, description, languages, feeds
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
  [`description`], [str or none], [`description` from the config, in the default language.],
  [`languages`], [array], [`(code, name)` dicts, default first. Empty unless #link("../write/i18n.typ")[i18n] is on.],
  [`feeds`], [array], [One `(format, mime, urls)` dict per format `generate { feed { formats } }` asks for. Empty when the build writes none.],
  [`feed-url`], [function], [`feed-url(feed, code)`: that feed's URL in one language, or `none` for a language the site does not build.],
)

A feed's name and its place both move with config, so link one through `feeds`
rather than writing `/rss.xml` by hand:

```typ
#import "@baudelaire/site:0.1.0": feed-url, feeds

#for feed in feeds {
  let url = feed-url(feed, page.lang)
  if url != none { h("a", href: url, upper(feed.format)) }
}
```

The autodiscovery `<link rel="alternate">` tags are baudelaire's own work in the
head, so this is for the links a *reader* clicks; writing one in a template is a
duplicate, and one that does not know the base URL.

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
  [`description`], [Its one-line summary, from `description` or the `summary` alias.],
  [`image`], [Its own social image, or `none`.],
  [`alt`], [What that image shows, or `none`. Empty marks it decorative.],
  [`author`], [Who wrote it, or `none`. The page's own; never the site default.],
  [`taxonomies`], [A dict of `taxonomy -> (terms..)`.],
  [`extra`], [Frontmatter baudelaire does not name: the theme's own keys.],
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

=== From a content page

A page may import the catalogue itself, which is how this site's home page
counts its own pages:

```typ
#import "@baudelaire/site:0.1.0": lang
#import "@baudelaire/pages:0.1.0": pages

This site has #pages(lang).len() pages.
```

The rendered page holds the real catalogue. Reading it while *frontmatter* is
being collected is the one thing it cannot do: the catalogue is built from the
frontmatter of every page, this one included, so at that moment it reads empty.
Deriving a `title`, a `slug`, or a `date` from `pages()` gets you nothing;
showing a count, a list, or a grid in the body works.

== sources

One binding per `paths { sources }` entry, holding the path that file is served
under. A page writes a *name*; `paths { sources }` is the only place a path is
written, so a page can reach a declared file and nothing else.

```typ
#import "@baudelaire/sources:0.1.0": changelog, data, logo
#import "@baudelaire/html:0.1.0": svg

#include changelog          // a `.typ` source is a body
#let counts = json(data)    // any other kind is data
#svg(logo)                  // including an icon to inline
```

A name is bound as written, so it has to be one Typst can bind: a letter or `_`
first, then letters, digits, `_` or `-`. Declaring one twice is refused rather
than resolved twice over.

The module is empty on a site that declares nothing, and importing a name that
was never declared is an import error rather than a `none`.

This is the only way to reach a file outside the project: `#include
"../CHANGELOG.typ"` cannot be written, because typst refuses a path outside its
root. Each declared file is mounted under a project path instead, so the
compiler opens it, tracks it as a dependency of the pages that read it, and
reports a fault inside it against the file itself. See
#link("../write/markdown.typ")[markdown pages] for the frontmatter `source` key,
which is the same declaration from the other side.

== markdown

`md` renders a chunk of markdown inside a Typst page, through the same parser a
#link("../write/markdown.typ")[`.md` page] goes through. A fragment and a page
agree about what a table, a list or a footnote becomes, and it reads this site's
own `content { markdown { extensions } }`.

````typ
#import "@baudelaire/markdown:0.1.0": md

#md("A **bold** claim and a [link](https://example.com).")

#md(```md
| Format | File     |
| ------ | -------- |
| RSS    | rss.xml  |
```)

#md(path: "notes.md")
````

#table(
  columns: 2,
  align: (left, left),
  table.header([Written as], [Is]),
  [a string], [The markdown, as written.],
  [a `raw` block], [Its text. Marking it ```` ```md ```` keeps editor highlighting.],
  [`path:`], [A file to read it from, resolved against the page. A leading `/` is the project root.],
)

#callout(kind: "warn")[
  Not a content block. `md[**bold**]` is parsed by *Typst* before `md` is
  reached, so the markdown is gone by then and `**bold**` would render as Typst
  markup. It is refused by name rather than silently rendered wrong.
]

A file read this way is an ordinary dependency, so editing it rebuilds the pages
that render it and nothing else. Keep fragments *outside* `content/`: a `.md`
file under there is a page in its own right, and would publish at its own URL as
well as appearing wherever you render it.

#callout(kind: "note")[
  Unlike every other module here, this one is not pure Typst underneath: `md`
  calls into baudelaire, because lowering markdown is not something Typst can do.
  A mirrored copy still resolves in an editor, and a plain `typst compile` of a
  page using it fails at the call saying so.
]

== toc

`toc` puts a table of contents where you place it, built from the headings of
the page it lands on.

```typ
#import "@baudelaire/toc:0.1.0": toc

#h("aside", class: "sidebar")[
  #h("h2")[On this page]
  #toc()
]
```

It emits a `<nav>` holding one nested list, each entry a link to a heading's
`id`:

```html
<nav>
  <ul>
    <li><a href="#install">Install</a>
      <ul><li><a href="#from-source">From source</a></li></ul>
    </li>
    <li><a href="#usage">Usage</a></li>
  </ul>
</nav>
```

#table(
  columns: 3,
  align: (left, left, left),
  table.header([Argument], [Default], [Is]),
  [`from`], [`2`], [The shallowest heading level listed.],
  [`to`], [`3`], [The deepest.],
  [`ordered`], [`false`], [`#true` builds `<ol>` instead of `<ul>`.],
  [anything else], [], [An attribute on the `<nav>`, exactly as in `h`.],
)

Typst's `= Section` is an `<h2>`, `== Subsection` an `<h3>`, so the default range
is the two levels a page written in the usual way has.

#callout(kind: "note")[
  A page's heading set only exists once it has compiled, so `toc()` cannot return
  the entries: it writes an empty `<nav>` and baudelaire fills it in afterwards.
  That is why there is no `page.headings`, and why the call takes no body.
]

Two things decide what is listed:

- *Where the heading is.* Only headings inside the element
  `html { region { element } }` names, minus the tags its `ignore` lists. A
  sidebar's own `On this page` heading sits outside `<main>`, so it never
  appears in its own table of contents.
- *Whether it has an `id`.* Nothing can link to a heading without one.
  `html { anchors }` gives every heading one by default; narrowing its `levels`
  or turning it off narrows what a table of contents can list too.

A page with no heading in range gets an empty `<nav>`, which `nav:empty { display: none }`
hides along with nothing else.
