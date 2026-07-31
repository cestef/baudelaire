# albatros

A centred blog theme, multilingual out of the box: one column at a comfortable
measure, the system type stack, light and dark, a language switcher built from
each page's own editions, and nothing on screen that a post did not put there.

```kdl
theme "themes/albatros"
```

## What you get

- `page.typ` — a post or page: title, byline (localized date · reading time),
  body, tag chips, prev/next pager.
- `home.typ` — a page's own words followed by the newest posts, read from the
  `@baudelaire/pages` catalogue. Bind it with `template: "home.typ"`.
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
| `collection` | on a `home.typ` page: which collection to list (default `posts`) |
| `recent` | on a `home.typ` page: how many to list (default 5) |

## Multiple languages

Declare the languages and write `post.fr.typ` beside `post.typ`. The switcher in
the header is built from `page.translations`, so it offers only the editions
that page actually has, and disappears entirely on a single-language site.

Every visible word comes from the site's own string table, so no template needs
editing to change language:

```kdl
languages {
  en { name "English" }
  fr {
    name "Français"
    strings {
      reading "min de lecture"
      tags "Étiquettes"
      newer "Plus récent"
      older "Plus ancien"
      recent "Derniers articles"
      archive "Tous les articles"
    }
  }
}
```

Keys used: `skip`, `primary`, `theme`, `languages`, `reading`, `tags`,
`pagination`, `previous`, `next`, `newer`, `older`, `recent`, `archive`. Dates
are localized by baudelaire itself, from the page's language.

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
