# spleen

A terminal. Monospace throughout, a prompt for a masthead, nav as commands, and
a post index laid out like a directory listing. Dark by default; a reader whose
system asks for light gets light. Multilingual sites get an `[en|fr]` switcher,
plain links like everything else here.

```kdl
theme "themes/spleen"
```

## No JavaScript

Not "minimal JavaScript": none. Colours come from `prefers-color-scheme`, the
listing is a grid, and the collapsible parts are `<details>` or absent. A page
built with this theme works with script disabled, and there is no toggle to
flash on load.

## What you get

- `page.typ` — a post: title as a comment line, one meta row (date · reading
  time · `[tags]`), body, prev/next.
- `list.typ` — every generated index. Dated listings get a fixed date column;
  the term index drops it rather than leaving a ragged gap.
- A `posts` collection at `/posts/{slug}/` with a paginated index of 20, a
  `tags` taxonomy, RSS and Atom, a sitemap, and `robots.txt`.

## What it expects from a page

Nothing mandatory beyond `title`. It uses `date`, `tags`, and `summary` when
they are there.

Listing dates render in ISO form (`2026-07-20`), which is deliberate: a
fixed-width column is the whole point of the layout. Post bylines use the
localized date, so the two forms are not in conflict.

## Translating it

Every visible word except the shell furniture comes from the site's string
table:

```kdl
languages { fr { strings { reading "min de lecture"; newer "récent"; older "ancien" } } }
```

Keys used: `skip`, `languages`, `reading`, `pagination`, `newer`, `older`.

## Overriding it

Copy any file into your own tree at the same relative path and yours wins. For
a recolour, restate the custom properties at the top of `style.css`:
`--accent` (prompt and links), `--accent-alt` (hover, hostname), `--dim`,
`--rule`, `--bg`, `--fg`, `--measure`.
