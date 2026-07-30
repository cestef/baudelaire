# Themes

Three themes for baudelaire, each a complete site look: templates, assets, and
config defaults. They are original work, not ports: the shapes are familiar
because the genres are (a centred blog, a terminal, a multilingual journal), but
every line of Typst and CSS here is ours and carries no upstream licence.

| Theme | Shape | Needs JS |
|---|---|---|
| [`albatros`](albatros) ([demo](https://baudelaire.cstef.dev/themes/albatros/)) | Centred blog. System sans, generous measure, light/dark toggle, tags, reading time. | one 20-line module for the toggle |
| [`spleen`](spleen) ([demo](https://baudelaire.cstef.dev/themes/spleen/)) | Terminal. Monospace throughout, boxed header, prompt-styled nav, dark-first. | no |
| [`voyage`](voyage) ([demo](https://baudelaire.cstef.dev/themes/voyage/)) | Multilingual journal. Serif headings, language switcher, section nav, UI strings from config. | one 20-line module for the toggle |

The demos are one site, [`demo/`](demo), built once per theme by
[`demo/build.sh`](demo/build.sh), which the docs workflow runs on every deploy.
Locally: `just previews`.

## Using one from this repository

A theme directory must sit inside the project that uses it, because a Typst
import cannot leave the project root. Copy or submodule it in, then name it:

```kdl
theme "themes/albatros"
```

## Using one as a package

Themes are ordinary Typst packages. Install one into the package data directory
under a namespace of your own and every project on the machine can name it
without a copy:

```bash
cp -r themes/albatros ~/.local/share/typst/packages/local/albatros/0.1.0
```

```kdl
theme "@local/albatros:0.1.0"
```

Published to Typst Universe, the same theme is `@preview/albatros:0.1.0`. Only
the `preview` namespace is ever downloaded; anything else is read from those
local directories.

## What a theme gives you, and what you keep

Everything a theme provides is a default:

- **Templates.** Your `templates/page.typ` wins over the theme's, file by file.
  Copy one out of the theme and edit it; nothing else changes.
- **Assets.** The theme's `assets/` are processed alongside yours. Shipping your
  own `style.css` replaces the theme's and keeps the rest.
- **Config.** `theme.kdl` is a floor. Every key you state wins, nested blocks
  included, so adopting a theme never touches your `site`, `url`, or `author`.

One sharp edge worth knowing: config *lists* replace wholesale. A site that
declares `content { collections { .. } }` replaces the theme's set rather than
adding to it, so copy the theme's collection block if you only meant to add one.

## Navigation

None of these themes hardcode a menu. The top nav is derived from
`@baudelaire/sections`, the build's own view of `content/`, so a new top-level
directory appears in the nav and a removed one disappears, with no config to
keep in sync.

## Writing your own

Start by copying the closest one. The layout is fixed (a theme cannot know what
you renamed your own directories to):

```text
templates/   layouts a page binds to, by filename
assets/      processed by the asset pipeline
static/      copied verbatim
theme.kdl    config defaults
typst.toml   package manifest, for publishing
```

Two rules that only bite theme authors:

- **Import siblings relatively.** Inside a theme, `#import "../parts.typ"`
  resolves in both modes; a root-absolute `/parts.typ` resolves against the
  *project* root when the theme is a directory and against the *package* root
  when it is published, so it cannot be right in both.
- **`svg()` is off limits.** Its paths are project-root absolute, and a theme
  does not know where it sits in your project. Build icons as inline `<svg>`
  elements instead, the way `parts.typ` does in each theme here.
