// The package entrypoint: the theme's building blocks, for a site that wants
// one of them inside a page rather than a whole layout.
//
//   #import "@preview/voyage:0.1.0": lang-switch, chips
//
// The layouts themselves are not re-exported: baudelaire loads those from
// `templates/` by filename, which is what makes them overridable file by file.

#import "parts.typ": byline, chips, icon, label, lang-name, lang-switch, moon, pager, posted, shell, sun, top-nav
