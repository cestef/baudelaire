#let frontmatter = (
  title: "Code highlighting",
  order: 5,
)
#import "/templates/theme.typ": callout

Fenced code blocks are highlighted by Typst itself. Register a grammar once with a `show` rule in your template and it applies to every page:

```typ
#show raw.where(lang: "powershell"): set raw(syntaxes: "/highlight/powershell.sublime-syntax")
```

`syntaxes` points at a Sublime-syntax grammar (`.sublime-syntax`). Typst ships grammars
for most common languages, and baudelaire adds `kdl`, the language its config is
written in, so you only need your own for niche ones. This site adds
`powershell.sublime-syntax` for the Windows install steps and nothing else.

#callout(kind: "note")[
  Typst builds a syntax set from the files you hand it and nothing else, so a grammar that reaches into another Sublime package (`embed: scope:source.regexp`, `set: scope:source.cs`) cannot resolve that reference, and the text it covers is dropped from the output rather than left unhighlighted. Vendoring a grammar from the wild usually means replacing those few contexts with plain handling.
]

== Colors that follow a dark-mode toggle

Typst's HTML export writes highlight colors inline, as a `color` style on every span, with no option to emit classes. A color fixed at build time cannot follow a toggle that flips in the browser.

With an `html { highlight }` block, baudelaire highlights code blocks itself and emits a class per token:

```kdl
html {
  highlight
}
```

```html
<span class="sx-keyword">let</span>
```

The palette then lives in your stylesheet:

```css
.sx-keyword { color: var(--sx-keyword); }
.sx-string  { color: var(--sx-string); }
.sx-comment { color: var(--sx-comment); font-style: italic; }

:root             { --sx-keyword: #a12a5e; }  /* light */
[data-theme=dark] { --sx-keyword: #e58fa6; }  /* dark  */
```

The `theme` a `show raw` rule sets is not read at all in this mode, and neither are the bold and italic a `.tmTheme` puts on a scope: styling is the stylesheet's, whole. A page that sets one anyway is told once that it is being ignored, since a discarded palette otherwise looks like code that simply came out unstyled.

== The vocabulary

Each grammar names its scopes differently (`keyword.other.fn.rust`, `keyword.control.import.python`). Baudelaire maps every grammar, and Typst's own parser, onto one fixed set of tokens, so `#let` in a `typ` block and `let` in a `rust` one are both `sx-keyword`:

#table(
  columns: 2,
  [*Token*], [*What lands there*],
  [`comment`], [Comments, and the `//` that opens one],
  [`string`], [Strings, quotes included],
  [`escape`], [`\n` and friends inside one],
  [`number`], [Numeric literals],
  [`constant`], [`true`, `nil`, named constants],
  [`keyword`], [Keywords, and `storage`: `let`, `fn`, `struct`],
  [`operator`], [`+`, `==`, `->`],
  [`punctuation`], [Brackets, separators, terminators],
  [`function`], [Definitions and calls],
  [`type`], [Type, class and struct names],
  [`namespace`], [Module and package names],
  [`tag`], [Markup element names],
  [`attribute`], [Markup attribute names],
  [`property`], [Object keys, struct members],
  [`variable`], [Everything else with a name],
  [`parameter`], [Names in a signature],
  [`heading`], [Headings, in markup languages],
  [`strong`], [Bold markup, and a term in a list],
  [`emph`], [Italic markup],
  [`link`], [Links],
  [`raw`], [Inline code, in markup languages],
  [`label`], [Labels and references],
  [`invalid`], [What the grammar could not parse],
)

Text no token claims gets no span.

== Naming and narrowing

Class names and the token set are configurable:

```kdl
html {
  highlight {
    // What every class starts with. Empty for none.
    prefix "tok-"
    // Drop the tokens you do not paint; `-name` removes, a bare name adds.
    tokens "-punctuation" "-variable"
    // Rename one, prefix aside: this writes `tok-kw`.
    classes {
      keyword "kw"
    }
    // Keep the grammar's own scope on the span, as `data-scope`.
    scopes #true
  }
}
```

A dropped token keeps its text and loses its span, so narrowing `tokens` is how a small palette gets small markup.

Use `scopes` when you need to tell one kind of keyword from another:

```css
[data-scope^="keyword.control"] { font-style: italic; }
```

Without the block the colors stay inline, as Typst emits them. Keep it that way if you want your own `.tmTheme` rendered as written.

== In a theme

All four #link("../theming/using.typ")[shipped themes] turn `highlight` on in their `theme.kdl` and define the palette in `style.css`. Each maps the twenty-odd tokens onto about eight color variables: tokens a reader has to tell apart get their own color, and the rest share the closest one.
