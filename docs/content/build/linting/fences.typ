#let frontmatter = (
  title: "Code fences",
  order: 2,
)
#import "/templates/theme.typ": callout

A fence claims a language. `snippets` is where a site says what that claim is
worth: nothing until a language is named, and from then on every fence of it is
read by something that knows the language.

```kdl
check {
  snippets {
    kdl  run="$BAUDELAIRE config check --isolated --compact {file}"
    json
    typ "warn"
    sh   run="shellcheck -s sh {file}"
  }
}
```

A line with no `run` is checked by the parser this binary already carries:

#table(
  columns: 2,
  align: (left, left),
  table.header([Language], [Read by]),
  [`kdl`], [The KDL parser, for syntax alone.],
  [`json`], [`serde_json`.],
  [`toml`], [`toml_edit`.],
  [`yaml`, `yml`], [`saphyr`.],
  [`typ`, `typst`], [Typst's own parser, which parses without evaluating.],
)

Naming any other language without a `run` is a config error rather than a line
that checks nothing.

#table(
  columns: 3,
  align: (left, left, left),
  table.header([Key], [Type], [Does]),
  [`run`], [str], [The command that checks one snippet. `{file}` is the snippet on disk, as one shell word, and `{lang}` the language it claimed. Nonzero exit is a finding; what the command wrote is the message.],
  [`hidden`], [str], [A line prefix that is checked and never shown, for the context a fragment needs. See #link(<hidden>)[hidden lines].],
)

A `run` command is run through the system shell in the project root, like a
#link("../hooks.typ")[hook], and `$BAUDELAIRE` is the binary running the build, so a
site checks its own config examples with the very build that renders them rather
than with whatever is on `PATH`. A command that reports `file:line:column:` has
its message placed on that line of the fence; one that reports anything else is
reported against the fence itself.

== Hidden lines <hidden>

A documentation fence usually shows a fragment: the block that matters, not the
three blocks around it. `hidden` is the prefix that carries the rest:

````typ
```kdl
//! content { collections { posts {
schema { title "str" }
//! } } }
```
````

The reader sees the `schema` block. The checker reads all five lines. The prefix
is yours to pick, and a comment in the language keeps the raw file readable to
anyone who never renders it.

One line is a directive rather than context: `//! @ignore` (the prefix, then
`@ignore`) says this fence is not meant to check at all, which is what a snippet
that is wrong on purpose needs.
