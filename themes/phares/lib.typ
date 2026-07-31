// The package entrypoint: the theme's building blocks, for a site that wants
// one of them inside a page rather than a whole layout.
//
//   #import "@preview/phares:0.1.0": callout, chips
//
// The layouts themselves are not re-exported: baudelaire loads those from
// `templates/` by filename, which is what makes them overridable file by file.

#import "parts.typ": (
  chips, contents, icon, label, magnifier, menu, moon, pager, search-trigger, shell, sidebar, sun,
  theme-toggle,
)

#import "@baudelaire/html:0.1.0": h

// An aside a documentation page can drop into its own prose:
//
//   #callout(kind: "warning")[This deletes the index.]
//
// `kind` is one of note, tip, warning, danger; anything else still renders and
// takes the default colours, so a site can invent its own and style it.
#let callout(body, kind: "note", title: none) = h("aside", class: "callout callout-" + kind, {
  if title != none { h("p", class: "callout-title", title) }
  body
})
