#let frontmatter = (
  title: "Code fences",
  order: 2,
)
#import "/templates/theme.typ": callout

A code fence names a language. `snippets` says how to check it: nothing is checked until a language is named, and then every fence in that language is checked by a tool that understands it.

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

Naming any other language without a `run` is a config error.

#table(
  columns: 3,
  align: (left, left, left),
  table.header([Key], [Type], [Does]),
  [`run`], [str], [The command that checks one snippet. `{file}` is the snippet on disk, as one shell word, and `{lang}` the language it claimed. Nonzero exit is a finding; what the command wrote is the message.],
  [`hidden`], [str], [A line prefix that is checked and never shown, for the context a fragment needs. See #link(<hidden>)[hidden lines].],
)

A `run` command is run through the system shell in the project root, like a
#link("../hooks.typ")[hook], and `$BAUDELAIRE` is the binary running the build, so a site checks its own config examples with the same build that renders them, not whatever is on `PATH`. A command that reports `file:line:column:` has
its message placed on that line of the fence; one that reports anything else is
reported against the fence itself.

== Hidden lines <hidden>

A documentation fence usually shows a fragment: the block that matters, without the blocks around it. `hidden` is the prefix for the missing context:

````typ
```kdl
//! content { collections { posts {
schema { title "str" }
//! } } }
```
````

The reader sees the `schema` block. The checker reads all five lines. Pick the prefix yourself; a comment in the language keeps the raw file readable.

One line is a directive, not context: `//! @ignore` (the prefix, then
`@ignore`) says this fence is not meant to check at all, which is what a snippet
that is wrong on purpose needs.
