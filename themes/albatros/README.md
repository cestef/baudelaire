# albatros

A centred blog theme with multilingual support. It has one column, the system
type stack, light and dark schemes, and a language switcher built from each
page's own editions.

```kdl
theme "themes/albatros"
```

## What you get

- `page.typ`: a post or page: title, byline (author · localized date · reading
  time), lead image, an optional contents list, body, tag chips, related posts,
  prev/next pager.
- `home.typ`: a page's own words followed by the newest posts, read from the
  `@baudelaire/pages` catalogue. Bind it with `template: "home.typ"`.
- `list.typ`: every generated index: the paginated collection index, `/tags/`,
  and each term page.
- `not-found.typ`: the page a host serves for an unmatched URL. Bind it from
  `content/404.typ`, which publishes as a flat `404.html`.
- A `posts` collection at `/posts/{slug}/` with a paginated index, a `tags`
  taxonomy with term pages, RSS and Atom, a sitemap, and heading anchors.

## What it expects from a page

Only `title` is required. It also uses these when present:

| Frontmatter | Effect |
|---|---|
| `date` | byline date, feed date, listing date |
| `tags` | chips under the post, term pages under `/tags/` |
| `summary` | one line under the entry in a listing |
| `image`, `alt` | the lead image over the post, the thumbnail in a listing, and the social card |
| `toc` | `true` draws the post's own contents above it, from its `h2`/`h3` |
| `author`, or a taxonomy with `credit "author"` | the byline, with the link and picture an entity carries |
| `collection` | on a `home.typ` page: which collection to list (default `posts`) |
| `recent` | on a `home.typ` page: how many to list (default 5) |

## Multiple languages

Declare the languages and write `post.fr.typ` beside `post.typ`. The switcher in
the header is built from `page.translations`, so it offers only the editions
that page has, and it is absent on a single-language site.

Every visible word comes from the site's string table, so changing language
needs no template edits:

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
`pagination`, `previous`, `next`, `newer`, `older`, `recent`, `archive`, `home`,
`related`, `contents`, `copy`, `copied`. Dates are localized by baudelaire
itself, from the page's language.

## Related posts

A post ends with the posts nearest it: same collection, ranked by how many tags
they share, three at most. A post with no tags gets none.

## Bylines

`author: "Zoe Quill"` in frontmatter gives a plain name. For a name that links
somewhere and has a picture, declare the people once and point a taxonomy at
them:

```kdl
content {
  entities {
    people {
      shape "person"
      sources { inline { zoe { name "Zoe Quill"; url "https://zoe.example"; avatar "/assets/zoe.png" } } }
    }
  }
  taxonomies {
    tags { listing { template "list.typ" } }
    authors { entities "people"; credit "author"; listing { template "list.typ" } }
  }
}
```

A post with `authors: ("zoe",)` is then credited to a person the whole build
knows: the byline, the head tags, the feed entry, and `/authors/zoe/` as their
archive. The `tags` block is repeated because a `taxonomies` block of your own
replaces the theme's whole set.

## The navigation

The top nav comes from `@baudelaire/sections`, the build's view of `content/`:
one link per top-level directory that holds pages. Add `content/notes/` and a
Notes link appears. It links `/<dir>/`, which exists when that collection
generates an index (this theme's `posts` does).

## Overriding it

Copy any file out of the theme into your own tree at the same relative path and
yours wins: `templates/page.typ` for the layout, `assets/style.css` for the
look. For a small tweak, restate the custom properties at the top of
`style.css` (`--measure`, `--accent`, `--fg`, `--bg`, `--rule`).
