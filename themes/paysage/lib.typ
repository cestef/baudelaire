// The package entrypoint: the theme's building blocks, for a site that wants
// one of them inside a page rather than a whole layout.
//
//   #import "@preview/paysage:0.1.0": work-grid, hero
//
// The layouts themselves are not re-exported: baudelaire loads those from
// `templates/` by filename, which is what makes them overridable file by file.

#import "parts.typ": (
  hero, icon, label, moon, next-project, posted, shell, sun, theme-toggle, top-nav, work-card,
  work-grid,
)
