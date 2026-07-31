// The package entrypoint: the theme's building blocks, for a site that wants
// one of them inside a page rather than a whole layout.
//
//   #import "@preview/albatros:0.1.0": chips, entry-row, posted
//
// The layouts themselves are not re-exported: baudelaire loads those from
// `templates/` by filename, which is what makes them overridable file by file.

#import "parts.typ": (
  byline, chips, entry-list, entry-row, icon, label, lang-switch, moon, pager, posted,
  reading-badge, shell, sun, theme-toggle, top-nav,
)
