# albatros

A centred blog theme: one column at a comfortable measure, the system type
stack, light and dark, and nothing on screen that a post did not put there.

```kdl
theme "themes/albatros"
```

## What you get

- `page.typ` — a post or page: title, byline (localized date · reading time),
  body, tag chips, prev/next pager.
- `list.typ` — every generated index: the paginated collection index, `/tags/`,
  and each term page.
- A `posts` collection at `/posts/{slug}/` with a paginated index, a `tags`
  taxonomy with term pages, RSS and Atom, a sitemap, and heading anchors.

## What it expects from a page

Nothing mandatory beyond `title`. It uses, when present:

| Frontmatter | Effect |
|---|---|
| `date` | byline date, feed date, listing date |
| `tags` | chips under the post, term pages under `/tags/` |
| `summary` | one line under the entry in a listing |

## Translating it

Every visible word comes from the site's own string table, so no template needs
editing to change language:

```kdl
languages {
  fr {
    strings {
      reading "min de lecture"
      tags "Étiquettes"
      newer "Plus récent"
      older "Plus ancien"
    }
  }
}
```

Keys used: `skip`, `reading`, `tags`, `pagination`, `newer`, `older`. Dates are
localized by baudelaire itself, from the page's language.

## The navigation

The top nav is derived from `@baudelaire/sections`, the build's own view of
`content/`: one link per top-level directory that holds pages. Add
`content/notes/` and a Notes link appears. It links `/<dir>/`, which exists when
that collection generates an index (this theme's `posts` does).

## Overriding it

Copy any file out of the theme into your own tree at the same relative path and
yours wins: `templates/page.typ` for the layout, `assets/style.css` for the
look. For a tweak rather than a rewrite, restate the custom properties at the
top of `style.css` (`--measure`, `--accent`, `--fg`, `--bg`, `--rule`).
