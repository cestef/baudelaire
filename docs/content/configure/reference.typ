#let frontmatter = (
  title: "Config reference",
  description: "Every key `config.kdl` accepts, with its value shape.",
  order: 3,
)
#import "/generated/reference.typ": entries
#import "/templates/theme.typ": callout

Every key the config accepts, in the order the parser declares them. Generated
from the parser itself, so it can't drift from what actually parses.

#callout(kind: "tip")[
  `baudelaire reference` prints this in the terminal, and
  `baudelaire reference assets.images` prints one subtree of it.
]

// A description is prose with `code spans` in it, and it arrives as a plain
// string. Splitting on the backticks and wrapping the odd parts in `raw` is
// what renders those spans; the even parts stay strings, which typst inserts
// verbatim rather than parsing as markup, so no character in a description can
// open markup it did not mean to.
#let prose(text) = {
  for (i, part) in text.split("`").enumerate() {
    if calc.rem(i, 2) == 1 { raw(part) } else { part }
  }
}

// The generated module is flat: every key in declaration order, each carrying
// how deep it sits and whether it opens a block of its own. A block becomes a
// heading and the keys directly under it become that heading's table, so the
// page renders the nesting without the generator knowing anything about markup.
#let children(section) = entries.filter(entry => (
  not entry.section
    and entry.depth == section.depth + 1
    and entry.path.starts-with(section.path + ".")
))

#let keys(rows) = if rows.len() > 0 {
  table(
    columns: 3,
    align: (left, left, left),
    table.header([Key], [Shape], [Does]),
    ..rows.map(entry => (raw(entry.key), raw(entry.shape), prose(entry.doc))).flatten(),
  )
}

#keys(entries.filter(entry => entry.depth == 0 and not entry.section))

#for section in entries.filter(entry => entry.section) {
  heading(level: section.depth + 2, raw(section.path))
  prose(section.doc)
  parbreak()
  keys(children(section))
}
