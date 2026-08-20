# phares

A documentation theme. A sidebar built from your own `content/` tree, a search
palette on `/` or `⌘K`, the page's headings down the right with the section you
are reading marked, and prev/next that runs the length of the manual.

```kdl
theme "themes/phares"
```

## What you get

- `page.typ` — one documentation page: breadcrumbs, title, optional lead, body,
  tag chips, an edit link and a last-updated line, prev/next.
- `list.typ` — the taxonomy indexes (`/tags/` and each term).
- `not-found.typ` — the page a host serves for an unmatched URL, with the
  sidebar still beside it. Bind it from `content/404.typ`, which publishes as a
  flat `404.html`.
- One `docs` collection over everything in a subdirectory, so the manual is one
  document and prev/next crosses directory boundaries. Pages keep their natural
  URLs: `content/guide/install.typ` publishes at `/guide/install/`. Pages
  directly under `content/` stay where they are and get the same layout.
- A search index and its palette client, a sitemap, `robots.txt`, and
  `llms.txt`.

## Writing the manual

Directories are the sections. `content/guide/`, `content/reference/`, and
`content/guide/advanced/` nest in the sidebar exactly as they nest on disk,
which is `@baudelaire/sections`, the build's own view of the tree.

Order is `order` in frontmatter, and pages without one fall back to source path,
which is already the order a directory reads in. Number the pages that need to
lead:

```typ
#let frontmatter = (
  title: "Installation",
  order: 1,
  summary: "Three ways to get the binary.",
)
```

| Frontmatter | Effect |
|---|---|
| `order` | position in the sidebar and in prev/next |
| `summary` | the lead paragraph under the title, and the search snippet |
| `tags` | chips under the page, term pages under `/tags/` |

A directory's own name is titlecased for its sidebar heading (`getting-started`
→ `Getting started`). To call it something else, add a string under that id:

```kdl
languages { en { strings { getting-started "Start here" } } }
```

## Writing with it

The package exports the pieces a manual is made of. Import what a page needs:

```typ
#import "@preview/phares:0.1.0": badge, callout, card, cards, pane, steps, tabs
```

### Callouts

```typ
#callout(kind: "warning", title: "Careful")[This rewrites the index.]
```

`kind` is `note` (default), `tip`, `warning`, or `danger`. An unknown kind still
renders and takes the default colours, so you can invent one and style it.

### Tabs

```typ
#tabs(
  pane("cargo")[```sh cargo install wren --locked```],
  pane("brew")[```sh brew install wren```],
)
```

The script builds the strip. Without it every pane is simply visible, one after
another: nothing is hidden behind a control that did not arrive.

### Steps

```typ
#steps[
  + Put the binary on your `PATH`.
  + Run `wren init`.
]
```

The numbering is the list's own, so a step holds anything a list item can: a
code block, a callout, a nested list.

### Cards

```typ
#cards(
  card("Install", href: "/guide/install/")[Three ways to get the binary.],
  card("Writing", href: "/guide/writing/")[Files and frontmatter.],
)
```

### Badges

```typ
== Ranges #badge("0.2+")
```

`kind` shares the callout palette, so a warning badge and a warning callout are
the same colour.

## Breadcrumbs, dates, and an edit link

Breadcrumbs come from the page's own URL, with the site's string table naming
each directory. A crumb links only where a page is actually published, so the
middle of the trail is plain text on a manual whose directories have no index
page of their own, and never a dead link.

`updated: datetime(..)` in frontmatter prints a last-updated line under the
page. It is written as the ISO day: baudelaire localizes `date`, not this one,
and a manual would rather be language-neutral than wrong.

Beside it, a link to the page's own file. Say where the manual lives and the
theme appends `page.source`, which is project-root-absolute:

```kdl
typst {
  inputs {
    edit "https://github.com/you/site/edit/main"
  }
}
```

It is a `typst { inputs }` constant rather than a config key of this theme's
invention: a site's own constants belong to the site. Without one there is no
link, and a generated page has no file to offer.

## What the script does

Three things the layout cannot know, all of them from the rendered page:

- marks the sidebar link for the current URL and opens the groups above it
- builds the on-page contents from the headings that ended up in the body, and
  highlights the one being read
- remembers which groups you collapsed

The search palette is not part of it: `generate { search { ui } }` emits a
self-mounting client at `/search.js`, and this theme only restyles its `.bd-*`
classes.

## Translating it

Every visible word comes from the site's string table:

```kdl
languages {
  fr {
    strings {
      search "Rechercher"
      contents "Sur cette page"
      previous "Précédent"
      next "Suivant"
    }
  }
}
```

Keys used: `skip`, `search`, `theme`, `navigation`, `documentation`, `contents`,
`tags`, `pagination`, `previous`, `next`, `built`, `copy`, `copied`,
`breadcrumb`, `home`, `updated`, `edit`, plus any directory id you want
renamed.

## Overriding it

Copy any file into your own tree at the same relative path and yours wins. For a
recolour, restate the custom properties at the top of `style.css`: `--accent`,
`--fg`, `--bg`, `--raised`, `--rule`, and the widths `--sidebar`, `--toc`,
`--measure`.
