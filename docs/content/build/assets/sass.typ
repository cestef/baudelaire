#let frontmatter = (
  title: "Sass",
  order: 1,
)
#import "/templates/theme.typ": callout

A `.scss` or `.sass` file under the asset tree is a stylesheet. It is compiled
with #link("https://github.com/connorskees/grass")[grass], served as `.css`, and
goes on through everything a hand-written sheet gets: `minify`, `targets`,
`url()` rewriting, `fingerprint`, `integrity`, `embed`.

```text
assets/
  _vars.scss    -> nothing; the sheet that imports it carries it
  style.scss    -> /assets/style.css
```

Sass is not in the released binaries. Build one that has it:

```sh
cargo install baudelaire --features sass
```

A binary without it leaves a `.scss` where it lies and warns that it did.

```scss
// assets/style.scss
@use "vars";

.card {
  color: vars.$brand;
  .title { font-weight: 600 }
}
```

Link either spelling. `/assets/style.scss` is the file you put there and
`/assets/style.css` is what it became, and both resolve to the same compiled,
fingerprinted file:

```typ
#html.elem("link", attrs: (rel: "stylesheet", href: "/assets/style.scss"))
```

`@use` and `@import` resolve beside the importing sheet first, then across the
asset roots (the project's tree, then the theme's), so a sheet you wrote can
pull in a partial your theme ships, and a partial you add under the same name
wins.

#table(
  columns: 2,
  align: (left, left),
  table.header([Detail], [Is]),
  [Syntax], [Both. `.scss` is the CSS-like one, `.sass` the indented one, picked off the extension.],
  [Partials], [`_vars.scss` is import-only, as everywhere else in the tree.],
  [`@warn` / `@debug`], [Written to stderr as the sheet asked.],
  [Source maps], [Map the compiled CSS, not the Sass: grass emits no mapping to chain back to.],
  [Output], [Always expanded. `minify` is Lightning CSS's pass, one step later.],
)
