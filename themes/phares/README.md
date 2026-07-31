# phares

A documentation theme. A sidebar built from your own `content/` tree, a search
palette on `/` or `⌘K`, the page's headings down the right with the section you
are reading marked, and prev/next that runs the length of the manual.

```kdl
theme "themes/phares"
```

## What you get

- `page.typ` — one documentation page: title, optional lead, body, tag chips,
  prev/next.
- `list.typ` — the taxonomy indexes (`/tags/` and each term).
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

## Callouts

The package exports one, for the asides a manual needs:

```typ
#import "@preview/phares:0.1.0": callout

#callout(kind: "warning", title: "Careful")[This rewrites the index.]
```

`kind` is `note` (default), `tip`, `warning`, or `danger`. An unknown kind still
renders and takes the default colours, so you can invent one and style it.

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
`tags`, `pagination`, `previous`, `next`, `built`, plus any directory id you
want renamed.

## Overriding it

Copy any file into your own tree at the same relative path and yours wins. For a
recolour, restate the custom properties at the top of `style.css`: `--accent`,
`--fg`, `--bg`, `--raised`, `--rule`, and the widths `--sidebar`, `--toc`,
`--measure`.
