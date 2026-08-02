#let frontmatter = (
  title: "Code highlighting",
  order: 5,
)
#import "/templates/theme.typ": callout

Fenced code blocks are highlighted by Typst itself. Register a grammar and a theme once with a `show` rule in your template and it applies to every page:

```typ
#show raw.where(lang: "kdl"): set raw(syntaxes: "/highlight/kdl.sublime-syntax")
#show raw: set raw(theme: "/highlight/baudelaire.tmTheme")
```

`syntaxes` points at a Sublime-syntax grammar (`.sublime-syntax`), `theme` at a TextMate theme (`.tmTheme`). Typst ships grammars for most common languages, so you only need your own for niche ones. This site adds `kdl.sublime-syntax` for its config examples.

== Colors that follow a dark-mode toggle

Typst's HTML export bakes highlight colors *inline*, a `color` style on every span, with no option to emit CSS classes. A color frozen at build time cannot follow a toggle that flips at runtime.

Add an `html { highlight { } }` block and baudelaire rewrites those inline colors into classes, so the palette lives in your stylesheet where the toggle can reach it:

```kdl
html {
  highlight {
    keyword "#e5d004"
    string  "#e5d002"
    comment "#e5d001"
  }
}
```

Each entry names a color your `.tmTheme` paints, and the span carrying it comes out as `class="sx-keyword"`. Which means the theme's colors can be arbitrary *sentinels*: unique hex values standing for a scope rather than for a color.

```css
.sx-keyword { color: var(--sx-keyword); }
.sx-string  { color: var(--sx-string); }
.sx-comment { color: var(--sx-comment); font-style: italic; }

:root             { --sx-keyword: #a12a5e; }  /* light */
[data-theme=dark] { --sx-keyword: #e58fa6; }  /* dark  */
```

A color you do not name still becomes a class, keyed by its hex (`class="sx-e5d005"`), so a bare `html { highlight }` is enough to get out of inline styles entirely.

#callout(kind: "note")[
  Only spans inside a `pre` are rewritten. An inline color elsewhere is your own `#text(fill: ..)` and is left alone. Any other declaration in the same `style` survives too, so a theme's `font-style` on a scope still lands.
]

Without the block nothing changes: the colors stay inline, exactly as Typst emits them.

== In a theme

All four #link("../start/themes.typ")[shipped themes] pair a `highlight/palette.tmTheme` of sentinels with a `highlight` block in their `theme.kdl`, and define the real palette in `style.css`. The two files belong together: ship the palette without the block and the sentinel hexes reach the page as themselves.

A theme's `show raw` rule uses a *relative* path (`highlight/palette.tmTheme`), not a root-absolute one, because a theme resolves `/` against the project when it is a directory and against the package when it is installed. See #link("theme-authoring.typ")[writing a theme].
