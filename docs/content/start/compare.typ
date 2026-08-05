#let frontmatter = (
  title: "Compared",
  order: 4,
)
#import "/templates/theme.typ": callout

Every generator turns a content tree into HTML. What differs is the language a
page is written in, what ships inside the binary, and what you reach for a plugin
to get.

== The shape of each

#table(
  columns: 4,
  align: (left, left, left, left),
  table.header([Tool], [A page is], [A layout is], [Needs]),
  [Baudelaire], [Typst or Markdown], [a Typst function], [one binary],
  [Zola], [Markdown + TOML frontmatter], [a Tera template], [one binary],
  [Hugo], [Markdown + YAML/TOML frontmatter], [a Go template], [one binary],
  [Eleventy], [Markdown, Nunjucks, Liquid, JS], [any of those], [Node],
  [Astro], [Markdown, MDX, `.astro`], [a component], [Node],
  [Jekyll], [Markdown + YAML frontmatter], [a Liquid template], [Ruby],
  [mdBook], [Markdown], [a Handlebars template], [one binary],
)

Baudelaire is the row where the content language and the template language are
the same language, and that is most of the difference below.

== The page is a program

Markdown has no variables, no functions, and no way to read a file, so every
generator built on it grows a second layer for the moment you want one. Zola and
Hugo call it a shortcode, Eleventy a filter or a paired shortcode, Astro an MDX
component.

```html
<!-- Zola: templates/shortcodes/sales.html, then in the post -->
{{ sales(file="sales.csv") }}
```

In baudelaire the page is a Typst module, so the thing you would register is just
a binding:

```typ
#let rows = csv("/data/sales.csv")

#table(
  columns: 2,
  table.header([Quarter], [Revenue]),
  ..rows.slice(1).flatten().map(c => [#c]),
)
```

Nothing is registered, nothing is discovered by directory name, and the failure
mode is a compile error with a span rather than an empty div. The same applies to
what Markdown ecosystems ship as plugins: math, figures with numbering,
footnotes, bibliographies and cross references are Typst's, not baudelaire's.

Because a page compiles rather than expands, one source reaches more than one
target. `generate { pdf }` typesets the same page as a document, and
`generate { cards }` draws its social card with a Typst template, both from the
page you already wrote.

== What ships in the box

#table(
  columns: 6,
  align: (left, left, left, left, left, left),
  table.header([], [Baudelaire], [Zola], [Hugo], [Eleventy], [Astro]),
  [Taxonomies], [built in], [built in], [built in], [by hand], [by hand],
  [Pagination], [built in], [built in], [built in], [built in], [built in],
  [Feeds, sitemap], [built in], [built in], [built in], [plugin], [integration],
  [Search index], [built in], [built in], [by hand], [plugin], [plugin],
  [Multiple languages], [built in], [built in], [built in], [plugin], [built in],
  [CSS], [minified], [Sass], [Sass, PostCSS], [plugin], [Vite],
  [JS bundling], [rolldown], [none], [esbuild], [plugin], [Vite],
  [Fingerprint + SRI], [built in], [cachebust], [built in], [plugin], [built in],
  [Image variants], [built in], [built in], [built in], [plugin], [built in],
  [Link checking], [built in], [built in], [partial], [plugin], [plugin],
  [Accessibility lint], [built in], [none], [none], [plugin], [dev audit],
  [Byte budgets], [built in], [none], [none], [plugin], [plugin],
  [Upload command], [S3, SSH], [none], [S3, GCS, Azure], [none], [adapter],
  [PDF output], [built in], [none], [none], [none], [none],
  [Plugin API], [none], [none], [modules], [plugins], [integrations],
)

The last row is the trade. Baudelaire has no plugin interface: a capability is
either a switch in `config.kdl`, a Typst package you import, or a command in
`hooks { }`. That keeps the binary one file and the config exhaustive, and it
means a capability nobody has built does not exist yet.

== Builds

Baudelaire keeps an on-disk cache under `.baudelaire/` and rebuilds a page when
something that page actually read changed: its own source, an import, an asset it
named, a permalink one of its links resolves to. See
#link("../build/incremental.typ")[incremental builds]. Zola rebuilds from cold
every run. Hugo caches processed resources but recompiles pages. Eleventy has
`--incremental`, Astro leans on Vite's cache.

A warm build here is proportional to what you touched, not to how many pages you
have. A cold one is a full Typst compile per page, which is slower per page than
a Markdown parse, so a first build of thousands of pages is not where baudelaire
wins.

== What you give up

*Markdown as the primary format.* A `.md` file under `content/` is a page, with
frontmatter in YAML, TOML or KDL, so an existing corpus builds as it stands. But
Typst is the format the rest of the tool is shaped around: a `.md` page is
lowered to Typst before it compiles, and anything Typst does that Markdown has no
spelling for is reached through a fenced `typ` block rather than natively. See
#link("migrate/content.typ")[Markdown to Typst].

*Maturity of the target.* Typst's HTML export is the newest part of Typst, and
baudelaire pins one Typst version at a time. A page that leans on paged layout
(columns, placement, page breaks) has no HTML meaning, and an upgrade can move
the emitted markup.

*The ecosystem.* Four themes ship, there is no theme marketplace, no shortcode
registry, and no plugin API. Typst's own package registry is available to pages
and templates, which covers a lot of the gap for content but nothing for the
build.

*Sass.* `assets { minify }` runs the CSS through Lightning CSS. There is no Sass
or PostCSS step; run one from `hooks { before }` if you want it.

*HTML minification.* `html { pretty }` controls indentation, not size. Nothing
strips the markup.

*Components and islands.* Client JavaScript is yours to write and gets bundled.
There is no component model, no framework integration, no server rendering.

== Pick something else if

- Your site is really an application: interactive UI, framework components, SSR.
- Non-technical authors write the content and expect a CMS.
- You need a theme you can install today rather than a layout you write.

#callout(kind: "note")[
  A migration is not all or nothing at the URL level. Keeping the old paths alive
  is a frontmatter key, so a section can move over at a time. See
  #link("migrate/urls.typ")[keeping your URLs].
]

Ready to move a site? There is a page per source:
#link("migrate/from-zola.typ")[Zola], #link("migrate/from-hugo.typ")[Hugo],
#link("migrate/from-jekyll.typ")[Jekyll or Eleventy].
