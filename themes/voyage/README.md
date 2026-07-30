# voyage

A journal, multilingual first: serif headings against a sans body, a language
switcher in the header, and every visible word read from the site's own string
table. On a single-language site the switcher renders nothing and the theme is
an ordinary blog.

```kdl
theme "themes/voyage"
```

## The multilingual parts

- **Switcher.** Built from `page.translations`, which includes the page's own
  edition, so the active language is marked rather than dropped and a reader
  never lands on a switch that changes the subject. A page with no translation
  shows no switcher.
- **Names.** Taken from `languages { fr { name "Français" } }`, falling back to
  the uppercased code.
- **Nav.** `sections(page.lang)` never crosses languages, so a French page's nav
  links French pages.
- **Dates.** Written the way the page's language writes them, by baudelaire, not
  by this theme: typst's own `datetime.display` knows English month names only.
- **`hreflang`.** Emitted into the head by baudelaire itself. The switcher is a
  visible convenience, not a duplicate of it.

```kdl
lang "en"
languages {
  en { name "English" }
  fr {
    name "Français"
    strings {
      reading "min de lecture"
      previous "Précédent"
      next "Suivant"
      tags "Étiquettes"
      languages "Langues"
      // Dates are baudelaire's to render, and it carries no locale database:
      // a language that names neither falls back to `July 20, 2026`.
      date "{day} {month} {year}"
      months "janvier" "février" "mars" "avril" "mai" "juin" \
             "juillet" "août" "septembre" "octobre" "novembre" "décembre"
    }
  }
}
```

Keys used: `skip`, `reading`, `tags`, `languages`, `pagination`, `previous`,
`next`, `newer`, `older`.

## What you get

- `page.typ` — a post: title, byline (date · reading time), body, tags,
  prev/next with labelled titles.
- `list.typ` — every generated index, with dates, summaries, and each entry's
  tags.
- A `posts` collection at `/posts/{slug}/` with a paginated index, a `tags`
  taxonomy, RSS and Atom, and a sitemap. No `languages` block: a theme cannot
  know which languages a site speaks, and shipping one would turn i18n on for
  everybody.

## Overriding it

Copy any file into your own tree at the same relative path and yours wins. For
a recolour, restate the custom properties at the top of `style.css`:
`--accent`, `--fg`, `--bg`, `--raised`, `--rule`, `--measure`. The type stacks
are two declarations (`.brand, h1, h2, h3, .entry-title` for the serif, `body`
for the sans).
