# spleen

A terminal theme. It is monospace throughout, with a prompt as the masthead, nav
as commands, and a post index laid out like a directory listing. It is dark by
default, and light when the reader's system asks for light. Multilingual sites
get an `[en|fr]` switcher made of plain links.

```kdl
theme "themes/spleen"
```

## No JavaScript

This theme ships no JavaScript. Colours come from `prefers-color-scheme`, the
listing is a grid, and the collapsible parts are `<details>` or absent. Pages
work with script disabled, and there is no toggle to flash on load.

## What you get

- `page.typ`: a post: title as a comment line, one meta row (author · date ·
  reading time · `[tags]`), lead image, body, prev/next.
- `list.typ`: every generated index. Dated listings get a fixed date column;
  the term index drops it rather than leaving a ragged gap.
- `not-found.typ`: a shell answering a path that is not there, then `ls /`.
  Bind it from `content/404.typ`, which publishes as a flat `404.html`.
- A `posts` collection at `/posts/{slug}/` with a paginated index of 20, a
  `tags` taxonomy, RSS and Atom, a sitemap, and `robots.txt`.

## What it expects from a page

Only `title` is required. It also uses `date`, `tags`, `summary`, `image`
(with `alt`) and `author` when present. `image` is the lead picture over
the post and the one the social card names; `author`, or a taxonomy carrying
`credit "author"`, is the name at the head of the meta row.

Listing dates render in ISO form (`2026-07-20`) so the date column has a fixed
width. Post bylines use the localized date.

## Translating it

Every visible word except the shell furniture comes from the site's string
table:

```kdl
languages { fr { strings { reading "min de lecture"; newer "récent"; older "ancien" } } }
```

Keys used: `skip`, `languages`, `reading`, `pagination`, `newer`, `older`.

## Overriding it

Copy any file into your own tree at the same relative path and yours wins. To
recolour, restate the custom properties at the top of `style.css`:
`--accent` (prompt and links), `--accent-alt` (hover, hostname), `--dim`,
`--rule`, `--bg`, `--fg`, `--measure`.
