#let frontmatter = (
  title: "Syntax highlighting",
  order: 7,
)
#import "/templates/theme.typ": callout

Fenced code blocks are highlighted by Typst itself. Register a language once with
a `show` rule and it applies everywhere:

```typ
#show raw.where(lang: "kdl"): set raw(syntaxes: "/highlight/kdl.sublime-syntax")
#show raw: set raw(theme: "/highlight/baudelaire.tmTheme")
```

`syntaxes` points at a Sublime-syntax grammar (`.sublime-syntax`); `theme` at a
TextMate theme (`.tmTheme`). Typst ships grammars for many common languages, so
you only need your own for niche ones. This site adds `kdl.sublime-syntax` for
its config examples.

== Theming, and dark mode

Typst's HTML export bakes highlight colours *inline*, a `color` style on every
`span`, with no option to emit CSS classes. A colour frozen at build time cannot
follow a light/dark toggle that flips at runtime.

Turn on `html { highlight { } }` and Baudelaire rewrites those inline colours
into classes, so the palette lives in your stylesheet where the toggle can reach
it:

```kdl
html {
  highlight {
    keyword "#e5d004"
    string  "#e5d002"
    comment "#e5d001"
  }
}
```

Each entry names a colour your `.tmTheme` paints, and the span carrying it comes
out as `class="sx-keyword"`. Which means the theme's colours can be arbitrary
*sentinels*: unique hex values standing for a scope rather than for a colour.

```css
.sx-keyword { color: var(--sx-keyword); }
.sx-string  { color: var(--sx-string); }
.sx-comment { color: var(--sx-comment); font-style: italic; }

:root             { --sx-keyword: #a12a5e; }  /* light */
[data-theme=dark] { --sx-keyword: #e58fa6; }  /* dark  */
```

A colour you do not name still becomes a class, keyed by its hex
(`class="sx-e5d005"`), so a bare `html { highlight }` is enough to get out of
inline styles entirely. Only spans inside a `<pre>` are rewritten: an inline
colour in prose is your own `#text(fill: ..)` and is left alone.

#callout(kind: "note")[
  Without the block, nothing changes: the colours stay inline, exactly as Typst
  emits them. This is what this site uses, and its whole palette, including the
  code on this page, comes from `style.css`.
]
