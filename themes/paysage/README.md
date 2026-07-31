# paysage

A portfolio. A landing page that says what you do, a grid of work that builds
itself from the projects you publish, and case studies with room for pictures.

```kdl
theme "themes/paysage"
```

## What you get

- `home.typ` — the landing page: a hero in your own words, then a selection of
  work read from the `@baudelaire/pages` catalogue. Bind it with
  `template: "home.typ"`.
- `project.typ` — one case study: title, a fact row (date, role, client,
  duration, stack), a cover image, the write-up, and a link to the next project.
- `page.typ` — an ordinary page: about, contact, colophon.
- `list.typ` — the `/work/` index and the `stack` term pages, as the same grid.
- A `work` collection over `content/work/`, newest first, a `stack` taxonomy,
  RSS, and a sitemap.

## A project

```typ
#let frontmatter = (
  title: "Ledger",
  date: datetime(year: 2026, month: 3, day: 1),
  summary: "A double-entry bookkeeping engine that a spreadsheet can read.",
  cover: "/static/work/ledger.jpg",
  role: "Design and build",
  client: "Self-directed",
  duration: "4 months",
  stack: ("rust", "sqlite"),
)

= What it does
...
```

| Frontmatter | Effect |
|---|---|
| `summary` | the line under the title, and under the card in the grid |
| `cover` | the card image and the page's own cover |
| `date` | ordering, the year on the card, the fact row |
| `role`, `client`, `duration` | the fact row, each shown only if set |
| `stack` | chips on the card and the page, term pages under `/stack/` |

## The landing page

```typ
#let frontmatter = (
  title: "I build tools for people who read numbers.",
  template: "home.typ",
  tagline: "Systems engineer, occasionally a designer.",
  links: (("Email", "mailto:you@example.com"), ("GitHub", "https://github.com/you")),
  selected: 6,
)
```

`selected` caps the grid (default 6) and `collection` picks a different one than
`work`. Everything under the cap is linked from `All work →`, which is the
generated index.

The top nav needs no menu in config: content directories come from
`@baudelaire/sections` and the pages beside your landing page come from the page
catalogue, so an `about.typ` appears by existing.

## Images

`cover` is a URL, not a Typst `image()`: it is used as an `<img src>` on the
card and on the page. Put the files under `static/` (copied verbatim) or
`assets/` (processed, and then referenced by their built path).

## Translating it

Every visible word comes from the site's string table:

```kdl
languages {
  fr {
    strings {
      selected "Travaux choisis"
      all-work "Tous les travaux"
      next-project "Projet suivant"
      role "Rôle"
      stack "Outils"
    }
  }
}
```

Keys used: `skip`, `primary`, `theme`, `selected`, `all-work`, `next-project`,
`date`, `role`, `client`, `duration`, `stack`, `pagination`, `newer`, `older`.

## Overriding it

Copy any file into your own tree at the same relative path and yours wins. For a
recolour, restate the custom properties at the top of `style.css`: `--accent`
(links, hover, code), `--bg`, `--raised` (cards), `--fg`, `--muted`, `--rule`,
and the widths `--measure` and `--wide`.
