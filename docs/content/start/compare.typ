#let frontmatter = (
  title: "Compared",
  order: 4,
)
#import "/templates/theme.typ": callout

Every generator turns a content tree into HTML. They differ in the language a
page is written in, what ships inside the binary, and what needs a plugin.

== Languages and runtimes

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

In baudelaire the content language and the template language are the same
language. Most of the differences below follow from that.

== The page is a program

Markdown has no variables, no functions, and no way to read a file, so every
generator built on it adds a second layer for those. Zola and Hugo call it a
shortcode, Eleventy a filter or a paired shortcode, Astro an MDX component.

```html
<!-- Zola: templates/shortcodes/sales.html, then in the post -->
{{ sales(file="sales.csv") }}
```

In baudelaire the page is a Typst module, so what you would register is a
binding:

```typ
#let rows = csv("/data/sales.csv")

#table(
  columns: 2,
  table.header([Quarter], [Revenue]),
  ..rows.slice(1).flatten().map(c => [#c]),
)
```

Nothing is registered or discovered by directory name, and a mistake is a
compile error pointing at the line, not an empty div. Math, numbered figures,
footnotes, bibliographies and cross references, which Markdown ecosystems ship
as plugins, come from Typst itself.

A page compiles, so one source can reach more than one target.
`artifacts { pdf }` typesets the same page as a document, and
`artifacts { cards }` draws its social card with a Typst template.

== Built-in features

#table(
  columns: 6,
  align: (left, left, left, left, left, left),
  table.header([], [Baudelaire], [Zola], [Hugo], [Eleventy], [Astro]),
  [Taxonomies], [built in], [built in], [built in], [by hand], [by hand],
  [Pagination], [built in], [built in], [built in], [built in], [built in],
  [Feeds, sitemap], [built in], [built in], [built in], [plugin], [integration],
  [Search index], [built in], [built in], [by hand], [plugin], [plugin],
  [Multiple languages], [built in], [built in], [built in], [plugin], [built in],
  [CSS], [Sass, Tailwind, minified], [Sass], [Sass, PostCSS], [plugin], [Vite],
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

The last row is the trade-off. Baudelaire has no plugin interface: a capability
is a switch in `config.kdl`, a Typst package you import, or a command in
`hooks { }`. The binary stays one file and the config lists everything, but a
capability nobody has built does not exist yet.

== Builds

Baudelaire keeps an on-disk cache under `.baudelaire/` and rebuilds a page when
something that page actually read changed: its own source, an import, an asset it
named, a permalink one of its links resolves to. See
#link("../build/incremental.typ")[incremental builds]. Zola rebuilds from cold
every run. Hugo caches processed resources but recompiles pages. Eleventy has
`--incremental`, and Astro uses Vite's cache.

A warm build takes time in proportion to what you touched. A cold one is a full
Typst compile per page, which is slower than a Markdown parse, so a first build
of thousands of pages is where baudelaire is weakest.

== What you give up

*Markdown as the primary format.* A `.md` file under `content/` is a page, with
frontmatter in YAML, TOML or KDL, so an existing corpus builds as it stands. But
the rest of the tool is built around Typst: a `.md` page is converted to Typst
before it compiles, and anything Markdown has no syntax for goes in a fenced
`typ` block. See
#link("migrate/content.typ")[Markdown to Typst].

*Maturity of the target.* Typst's HTML export is the newest part of Typst, and
baudelaire pins one Typst version at a time. Paged layout (columns, placement,
page breaks) has no HTML meaning, and an upgrade can change the emitted markup.

*The ecosystem.* Four themes ship. There is no theme marketplace, no shortcode
registry, and no plugin API. Typst's package registry is available to pages
and templates, which helps with content but not with the build.

*PostCSS.* Sass compiles in the build, `assets { tailwind }` generates the
utility sheet, and `assets { minify }` runs the result through Lightning CSS.
There is no PostCSS step, and no LESS or Stylus: run those from
`hooks { before }`.

*HTML minification.* `html { pretty }` controls indentation, not size. Nothing
strips the markup.

*Components and islands.* Client JavaScript is yours to write and gets bundled.
There is no component model, framework integration or server rendering.

== Pick something else if

- Your site is an application: interactive UI, framework components, SSR.
- Non-technical authors write the content and expect a CMS.
- You need a theme you can install today rather than a layout you write.

#callout(kind: "note")[
  You can migrate one section at a time. Keeping the old paths alive is a
  frontmatter key. See
  #link("migrate/urls.typ")[keeping your URLs].
]

To move a site, pick your source:
#link("migrate/from-zola.typ")[Zola], #link("migrate/from-hugo.typ")[Hugo],
#link("migrate/from-jekyll.typ")[Jekyll or Eleventy].
