#let frontmatter = (
  order: 10,
  title: "Typst virtual modules",
  tags: ("feature",),
)
#import "/templates/theme.typ": callout

Baudelaire serves a small set of Typst modules under the `@baudelaire`
namespace. Nothing for them exists on disk and nothing is downloaded: the
compiler asks for the package, and baudelaire answers from memory.

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

== Versions

The version in the specifier is not optional: Typst's package syntax requires
one. It tracks the module API, never baudelaire's own version, so
`@baudelaire/html:0.1.0` keeps working across releases and only changes when
what these modules export changes.

#callout(kind: "warning")[
  Editor tooling cannot resolve `@baudelaire/*`, because the packages exist only
  while baudelaire is compiling. Expect an unresolved-import squiggle in your
  editor on a template that builds fine.
]
