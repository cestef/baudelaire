#let frontmatter = (
  order: 10,
  title: "Typst virtual modules",
  tags: ("feature",),
)
#import "/templates/theme.typ": callout

Baudelaire serves a small set of Typst modules under the `@baudelaire`
namespace. Nothing is downloaded: the compiler asks for the package, and
baudelaire answers it directly.

There are three, all at version `0.1.0`:
#raw("@baudelaire/html") for markup, #raw("@baudelaire/site") for the site's
identity, and #raw("@baudelaire/sections") for its page tree.

```typ
#import "@baudelaire/html:0.1.0": h, classes
#import "@baudelaire/site:0.1.0": title, url

#h("a", class: "brand", href: "/")[#title]
```

They are the Typst counterpart of the
#link("../assets/js-modules.typ")[`baudelaire:*` JavaScript modules], and read
from the same build data, so a template and a bundle can never disagree about
what the site is called.

== #raw("@baudelaire/html")

`html.elem` is the honest way to emit an element, and it is wordy: the tag, an
`attrs:` dict wrapper, and string values for everything. `h` is the same thing
with the wrapper removed.

```typ
// before
#html.elem("button", attrs: (class: "icon-btn", type: "button"), body)
// after
#h("button", class: "icon-btn", type: "button", body)
```

Named arguments become attributes, positional ones become children. Hyphenated
names need no quoting in either form, since a Typst identifier may contain a
hyphen: write `aria-label: "Close"`, not `"aria-label": "Close"`.

Values follow what HTML actually wants, which removes most of the `if`s and
`str()` calls a template accumulates:

/ `true`: a bare boolean attribute, so `data-theme-toggle: true` writes
  `data-theme-toggle`.
/ `none` / `false`: the attribute is dropped, so `h("a", href: target)` is safe
  when `target` may be missing.
/ anything else: coerced, so `width: size` needs no `str(size)`.

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
  `none` and adding it to a string fails the build. `classes` exists to make
  that unrepresentable. An empty result is `none`, which `h` then omits, so you
  never get a stray `class=""`.
]

=== Inlining an SVG

`svg()` puts an SVG file's own markup into the page as real DOM:

```typ
#svg("/icons/search.svg", class: "icon", aria-hidden: "true")
```

That is not the same as `image("/icons/search.svg")`, which produces an `<img>`.
An `<img>` is an opaque replaced element: CSS cannot reach inside it, so an icon
drawn that way cannot inherit `currentColor`, cannot be recoloured by a dark-mode
toggle, and cannot carry your own `class` or ARIA attributes. Inlining is the
only way to get those. Use `image()` for photographs, `svg()` for icons.

The file's own root attributes fill in under yours, so one file serves every
call site and you override only what differs:

```typ
#svg("/icons/search.svg", width: 16, height: 16)   // the file's viewBox, your size
```

The path is from the project root and starts with `/`. It cannot be relative to
the calling template: baudelaire reads the file after the compile, not typst, so
there is no template to resolve it against. Any project file works, so icons can
live outside `assets/` and never be published as standalone files.

Comments, XML declarations, DOCTYPEs, and an editor's private namespaces
(Inkscape's `sodipodi:*`, `dc:title` inside `<metadata>`) are dropped on the way
in, so an unedited export costs nothing extra. `xlink:href` becomes plain `href`,
the SVG 2 spelling.

#callout(kind: "warning")[
  A file carrying a `<script>`, an `on*` handler, or a `javascript:` URL is
  refused. Inlining makes it part of the page, so it would execute with your
  origin, which is rarely what an icon set from a package is expected to do. Put
  the script in your template instead, where a reader can see it.

  A `<style>` inside the file *is* kept, because Illustrator exports rely on one.
  Its rules are confined to the icon, so an export's `.st0` cannot repaint a
  `.st0` elsewhere on your page.
]

Confining is why an icon with a stylesheet gains a `data-svg` attribute: each
rule is rewritten to match only inside that element.

```css
/* authored */   .st0{fill:#231F20}
/* emitted  */   :where([data-svg="051c4bdc"]) .st0{fill:#231F20}
```

The wrapper is `:where()`, which adds no specificity, so a confined rule loses to
your page CSS exactly as it did before. `@media`, `@supports`, `@container` and
`@layer` blocks are descended into; `@keyframes` and `@font-face` name their own
thing rather than selecting elements, so they are left alone. An icon with no
stylesheet gains no attribute.

A selector that targets the icon's own root element (`svg { .. }` inside the
file) is not confined to it and will not match, since the rules are rewritten as
descendants. Style the shapes instead.

== #raw("@baudelaire/site")

Site identity as plain bindings, rather than a chain of guarded `.at` reads into
`sys.inputs`:

```typ
#import "@baudelaire/site:0.1.0": version, title, url, lang, author, languages
```

Every name is always bound, and unset config reads as `none`, so a theme can
ask for `author` on a site that never set one. A name that does not exist fails
at the import instead of quietly reading back `none`.

Build metadata that changes between builds is deliberately *not* here. Read
`git` and `date` from #link("context.typ")[`sys.inputs.baudelaire`], where
baudelaire tracks which pages read which value and rebuilds only those. A copy
baked into a module would rebuild the whole site on every commit.

== #raw("@baudelaire/sections")

The site's own view of `content/`, as a tree, for building a nav that cannot
drift from the pages.

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
single-language site too) and returns that language's tree. Each node is
`(id, pages, children)`; see #link("../content/navigation.typ")[navigation] for
the shape and the ordering rules.

This one differs from the other two in a way worth knowing. Its content depends
on every page in the site, so serving it from memory would make every page
depend on every other page's title and URL, and one rename would recompile
everything. Baudelaire writes it to a file under `.baudelaire/` instead and
serves the module from there, so Typst records the import as an ordinary file
dependency and only the templates that actually import it rebuild when the tree
moves.

== Versions

*Every module is served at `0.1.0`, and that is the only version there is.*
Write `:0.1.0` on every `@baudelaire/*` import and stop thinking about it.

The version is not optional, because Typst's package syntax requires one. It
tracks the module API, never baudelaire's own version, so `@baudelaire/html:0.1.0`
keeps working across releases and would only move if what these modules export
changed. Asking for any other version fails at the import, naming the one that
exists, rather than reaching for the network.

#callout(kind: "warning")[
  Editor tooling cannot resolve `@baudelaire/*`, because the packages exist only
  while baudelaire is compiling. Expect an unresolved-import squiggle in your
  editor on a template that builds fine.
]
