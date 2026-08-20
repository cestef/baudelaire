// The package entrypoint: the theme's building blocks, for a site that wants
// one of them inside a page rather than a whole layout.
//
//   #import "@preview/phares:0.1.0": callout, cards, steps, tabs
//
// The layouts themselves are not re-exported: baudelaire loads those from
// `templates/` by filename, which is what makes them overridable file by file.

#import "parts.typ": (
  breadcrumbs, chips, contents, edit-link, icon, label, magnifier, menu, moon, page-meta, pager,
  search-trigger, shell, sidebar, sun, theme-toggle, updated,
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

// A short label beside a heading or in a table: a version, a status, a warning
// in one word.
//
//   == Ranges #badge("0.2+")
//
// `kind` shares the callout palette, so `badge(kind: "warning")` and a warning
// callout are the same colour.
#let badge(text, kind: "note") = h("span", class: "badge badge-" + kind, text)

// A numbered procedure, from an ordinary Typst list:
//
//   #steps[
//     + Install the binary.
//     + Run `baudelaire init`.
//   ]
//
// The numbering is the list's own, so a step can hold anything a list item can:
// a code block, a callout, a nested list.
#let steps(body) = h("div", class: "steps", body)

// One link in a `cards` grid. The body is the description under the title, and
// `href` is what the whole card links to.
//
//   #card("Install", href: "/guide/install/")[Three ways to get the binary.]
#let card(title, body, href: none) = h(
  if href == none { "div" } else { "a" },
  class: "doc-card",
  href: href,
  {
    h("span", class: "doc-card-title", title)
    h("span", class: "doc-card-body", body)
  },
)

// A grid of `card`s, for the landing page of a section: a manual's index page
// says what is in it and lets a reader pick.
//
//   #cards(card("Install", href: "/guide/install/")[..], card("Writing", ..)[..])
#let cards(..items) = h("div", class: "cards", items.pos().join())

// One alternative in a `tabs` set: a label and what it shows.
#let pane(label, body) = h("div", class: "tab", data-tab: label, body)

// Alternatives a reader picks between, one at a time:
//
//   #tabs(pane("cargo")[```sh cargo install baudelaire```], pane("brew")[..])
//
// The theme's script turns these into a tab strip. Without it every pane is
// simply visible, one after another, which is the honest fallback: nothing is
// hidden behind a control that is not there.
#let tabs(..panes) = h("div", class: "tabs", data-tabs: true, panes.pos().join())
