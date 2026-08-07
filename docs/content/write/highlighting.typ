#let frontmatter = (
  title: "Code highlighting",
  order: 6,
)
#import "/templates/theme.typ": callout

Fenced code blocks are highlighted by Typst itself. Register a grammar once with a `show` rule in your template and it applies to every page:

```typ
#show raw.where(lang: "kdl"): set raw(syntaxes: "/highlight/kdl.sublime-syntax")
```

`syntaxes` points at a Sublime-syntax grammar (`.sublime-syntax`). Typst ships grammars for most common languages, so you only need your own for niche ones. This site adds `kdl.sublime-syntax` for its config examples and `powershell.sublime-syntax` for the Windows install steps.

#callout(kind: "note")[
  Typst builds a syntax set from the files you hand it and nothing else, so a grammar that reaches into another Sublime package (`embed: scope:source.regexp`, `set: scope:source.cs`) cannot resolve that reference, and the text it covers is dropped from the output rather than left unhighlighted. Vendoring a grammar from the wild usually means replacing those few contexts with plain handling.
]

== Colors that follow a dark-mode toggle

Typst's HTML export bakes highlight colors *inline*, a `color` style on every span, with no option to emit classes. A color frozen at build time cannot follow a toggle that flips at runtime.

Add an `html { highlight }` block and baudelaire highlights code blocks itself, emitting a class per token instead:

```kdl
html {
  highlight
}
```

```html
<span class="sx-keyword">let</span>
```

The palette then lives in your stylesheet, where the toggle can reach it:

```css
.sx-keyword { color: var(--sx-keyword); }
.sx-string  { color: var(--sx-string); }
.sx-comment { color: var(--sx-comment); font-style: italic; }

:root             { --sx-keyword: #a12a5e; }  /* light */
[data-theme=dark] { --sx-keyword: #e58fa6; }  /* dark  */
```

The `theme` a `show raw` rule sets is not read at all in this mode, and neither are the bold and italic a `.tmTheme` puts on a scope: styling is the stylesheet's, whole.

== The vocabulary

A grammar's scopes are its own (`keyword.other.fn.rust`, `keyword.control.import.python`), and a stylesheet naming them would be written once per language. Every grammar funnels into one closed set instead, and so does Typst's own parser, so `#let` in a `typ` block and `let` in a `rust` one are both `sx-keyword`:

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

Text no token claims gets no span at all: a code block is mostly plain, and a span around every word would be markup nobody reads.

== Naming and narrowing

The block is where a site says what it wants on the page:

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

`scopes` is the way out of the vocabulary when you need to tell one kind of keyword from another:

```css
[data-scope^="keyword.control"] { font-style: italic; }
```

Without the block nothing changes: the colors stay inline, exactly as Typst emits them, which is also what a site wanting its own `.tmTheme` rendered as written should keep.

== In a theme

All four #link("../start/themes.typ")[shipped themes] turn `highlight` on in their `theme.kdl` and define the palette in `style.css`. Each maps the twenty-odd tokens onto about eight color variables: the ones a reader has to tell apart get a color of their own, and the rest join whichever is closest.
