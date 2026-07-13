#let frontmatter = (
  title: "Syntax highlighting",
  order: 6,
  tags: ("guide", "reference"),
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

== The dark-mode caveat

Typst's HTML export bakes highlight colours *inline*, a `color` style on every
`span`, with no option to emit CSS classes instead. A fixed theme therefore
can't follow a light/dark toggle: the colours are frozen at build time.

#callout(kind: "warn")[
  A `.tmTheme` with real colours will look wrong in one of your two modes. The
  colour is decided at build time, but your background changes at runtime.
]

The fix is to make the theme's colours *arbitrary sentinels*: unique hex values
that stand for a scope, not a colour. Then remap them in CSS, where they can be
theme-aware:

```
/* baudelaire.tmTheme: each scope gets a unique, meaningless hex */
comment  -> #e5d001
string   -> #e5d002
keyword  -> #e5d004
```

```css
/* style.css: swap each sentinel for a real, adaptive variable */
:root            { --sx-keyword: #a12a5e; }   /* light */
[data-theme=dark]{ --sx-keyword: #e58fa6; }   /* dark  */

pre code [style*="e5d004"] { color: var(--sx-keyword) !important; }
```

An `!important` rule beats Typst's inline style, so the real palette lives in
CSS and follows the theme toggle. The whole of this site's code, including this
page, is coloured this way.
