#let frontmatter = (
  title: "Writing a theme",
  order: 5,
)
#import "/templates/theme.typ": callout

A theme packages templates, assets, and config defaults so a site gets all three by naming one line. Start by copying the closest of the #link("../start/themes.typ")[four shipped themes] and editing it in place.

```kdl
theme "themes/plume"
```

Point `theme` at a directory and it layers exactly as an installed package would, so the whole development loop is `just serve` on a real site.

#callout(kind: "warn")[
  The directory has to sit inside the project. A Typst import cannot reach outside the project root, so a theme kept elsewhere resolves its assets and then fails on its first template.
]

== Layout

Fixed directory names, because a theme cannot know what you renamed your own `paths` to:

#table(
  columns: 2,
  align: (left, left),
  table.header([Path], [Holds]),
  [`templates/`], [Layouts a page binds to, by filename.],
  [`assets/`], [Stylesheets, scripts, and images, run through the asset pipeline.],
  [`static/`], [Files copied to the output verbatim.],
  [`theme.kdl`], [Config defaults.],
  [`typst.toml`], [Package manifest, for publishing.],
  [`lib.typ`], [What a page can import from the theme.],
)

Anything else in the theme root is yours: the shipped themes keep a `parts.typ` of shared components and a `highlight/palette.tmTheme` there.

== Your file wins

The rule is the same for all four layered things, and it is the only rule.

- *Templates*: a page bound to `page.typ` uses the project's `templates/page.typ` when it has one, and the theme's otherwise. File by file.
- *Assets*: the theme's `assets/` are processed alongside yours, file by file. Ship your own `reset.css` and you get the theme's stylesheet with your reset.
- *Static*: the same, for files copied verbatim.
- *Config*: `theme.kdl` is a floor. Every key the site states wins; every key it leaves out falls back to the theme's, nested blocks included.

#callout(kind: "warn")[
  Lists replace wholesale. A site declaring `content { collections { .. } }` of its own drops the theme's set rather than adding to it, so a theme that defines collections should say so in its README and show the block to copy. See #link("../configure/profiles.typ")[the merge policy].
]

Because config is only a default, adopting a theme never touches a site's `site`, `url`, or `author`. A theme setting `html { pretty #false }` changes a build; a theme setting `site` does not, because the site set it too.

=== What a theme may not set

Five blocks are the site's alone, and `theme.kdl` is refused outright for naming any of them: `hooks`, `deploy`, `announce`, `paths`, and `profiles`. They decide what runs on the machine doing the building and where the result is sent, which is not a styling decision and not something a reader of your README would think to check. `hooks` is the sharp one: it runs commands through the system shell, so a theme that could set it would run code on every build of every site that adopted it, and for a package theme that code need not appear in the project at all.

Two keys inside blocks a theme *is* allowed are refused for the same reason: a `generate { headers { } }` rule, and a `redirect` whose old path carries a `*`. Both let a fetched theme speak to a browser in the site's name. A header rule sends any header on any path, and `Refresh` alone forwards every page somewhere else; a wildcard redirect claims no output file, so the check that stops a theme's redirect burying a real page has nothing to compare it against.

Turning the two rule files on stays yours: what goes in `_headers` is then computed from the site's own `caching` and `csp` blocks, and a literal `redirect` is still held to the collision check every redirect is.

If your theme needs a build step, say so in the README and give the block to copy. A theme is a floor for how a site *looks*, not for what its machine does.

== Three rules that bite theme authors

- *Import siblings relatively.* `#import "../parts.typ"` resolves both when the theme is a directory and when it is an installed package. A root-absolute `/parts.typ` resolves against the *project* root in the first case and the *package* root in the second, so it cannot be right in both. The same goes for a `show raw` theme path: write `highlight/palette.tmTheme`.
- *`svg()` is off limits.* Its paths are project-root absolute, and a theme does not know where it sits in a project. Build icons as inline SVG elements instead, which is what `parts.typ` does in every shipped theme.
- *Keep shared pieces out of `templates/`.* Only `templates/`, `assets/`, and `static/` are layered, so a `templates/parts.typ` can be shadowed by a project file that never meant to. At the theme root it cannot.

== Build the nav from the site

None of the shipped themes hardcode a menu, and yours should not either. Two #link("../lookup/typst-modules.typ")[virtual modules] hand you the build's own view of the site:

```typ
#import "@baudelaire/sections:0.1.0": sections
#import "@baudelaire/pages:0.1.0": pages

// every content directory that holds pages, nested
#let directories = sections(page.lang)
// every authored page of this language, as listing rows
#let posts = pages(page.lang).filter(p => p.collection == "posts")
```

A catalogue row is the same shape a generated listing hands its template as `entries`, so one card component renders a collection index, a term page, and a home-page grid. See #link("templates.typ")[templates] for the row fields.

== Every visible word

Read UI text off the site's own string table, so a non-English site translates your theme through config rather than by editing it:

```typ
#let label(page, key, fallback) = page.strings.at(key, default: fallback)

#label(page, "reading", "min read")
```

Dates arrive already localized: `page.date.display` is written the way the page's language writes one, and `page.date.iso` is what a machine wants. Never format one yourself, or a French page reads `July`. See #link("i18n.typ")[multiple languages].

== Publish it

`typst.toml` is an ordinary Typst package manifest, with `lib.typ` as its entrypoint:

```toml
[package]
name = "plume"
version = "0.1.0"
entrypoint = "lib.typ"
license = "MIT"
description = "A centered blog theme for baudelaire."
```

Export the theme's building blocks from `lib.typ`, for a site that wants one of them inside a page rather than a whole layout. Do not re-export the layouts themselves: baudelaire loads those from `templates/` by filename, and that is what makes them overridable file by file.

Install it into the Typst package directory to share it across projects without publishing anything:

```sh
cp -r themes/plume ~/.local/share/typst/packages/local/plume/0.1.0
```

```kdl
theme "@local/plume:0.1.0"
```

Only the `preview` namespace is ever downloaded. `@local` and every other namespace are read from the package directories on disk. Publish to `@preview` when it is ready and the change on a site using it is one line.

A build that cannot reach `packages.typst.org` names a mirror, which covers the theme and every `@preview` package a page imports:

```kdl
typst { registry "https://packages.example.net" }
```

A mirror redirects downloads only. Where a package lands, and where a local one is looked up, do not move.
