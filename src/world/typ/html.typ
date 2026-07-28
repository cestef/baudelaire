// `@baudelaire/html`: element construction without the ceremony.
//
// Generated and served by baudelaire; nothing for this exists on disk. Import
// it as `#import "@baudelaire/html:0.1.0": h, classes`.

// An attribute value as HTML sees it: a string.
//
// `true` writes a bare boolean attribute and `none`/`false` drop the attribute
// outright, so `h("a", href: target)` needs no surrounding `if`. Anything else
// is coerced, so a number does not need `str()`.
#let _value(value) = {
  if value == none or value == false { none } else if value == true {
    ""
  } else if type(value) == str { value } else { str(value) }
}

// An element, with named arguments as attributes and positional ones as
// children:
//
//   h("span", class: "entry-title")[#entry.label]
//   h("button", class: "icon-btn", type: "button", data-theme-toggle: true)[..]
//
// The same as `html.elem`, minus the `attrs:` wrapper, plus the value rules
// above. Hyphenated names need no quoting: a Typst identifier may contain them.
#let h(tag, ..args) = {
  let attrs = (:)
  for (name, value) in args.named() {
    let value = _value(value)
    if value != none { attrs.insert(name, value) }
  }
  html.elem(tag, attrs: attrs, args.pos().join())
}

// Join class names, dropping anything absent and taking a `(name, condition)`
// pair for a conditional one:
//
//   classes("callout", "callout-" + kind, ("active", is-current))
//
// `"a" + if cond { " b" }` yields `none` on the else branch and fails the
// build; this cannot. An empty result is `none`, which `h` then omits, so no
// `class=""` is ever written.
#let classes(..names) = {
  let out = ()
  for item in names.pos() {
    if item == none or item == false { continue }
    if type(item) == array {
      let (name, keep) = item
      if keep == true { out.push(name) }
    } else { out.push(item) }
  }
  out.join(" ")
}
