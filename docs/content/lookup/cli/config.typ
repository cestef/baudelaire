#let frontmatter = (
  title: "config",
  order: 1,
)
#import "/templates/theme.typ": callout

What can be said about a config without building the site it configures. Every
verb takes the global `-c` and `-p`, so a profile is applied by naming it there.

#table(
  columns: 3,
  align: (left, left, left),
  table.header([Verb], [Alias], [Does]),
  [`check [paths..]`], [`c`], [Parse it, resolve the theme it names and apply every profile it declares. `-` reads standard input.],
  [`explain <key>`], [`e`], [What one key is, what it holds, and which layer put it there.],
  [`get <key>`], [`g`], [What it holds, defaults included, on stdout for a script.],
  [`set <key> <value>`], [`s`], [Write one key back, the rest of the file exactly as authored.],
  [`show [key]`], [--], [The config, or one block of it, highlighted.],
)

```sh
baudelaire config check                          # would a build accept it?
baudelaire config check --isolated snippet.kdl   # a fragment, no project around it
baudelaire config explain content.collections.sort
baudelaire config set serve.port 4000
baudelaire config get paths.dist                 # public
baudelaire config get lint.headings.start --written   # only what the file says
baudelaire config show --effective lint          # every layer resolved, as KDL
```

A value is read out of the layers a config is built from, in the order they
apply:

#table(
  columns: 2,
  align: (left, left),
  table.header([Layer], [Is]),
  [`default`], [What a site holds before anything is written down.],
  [`theme`], [The theme's own `theme.kdl`, the base a site starts from.],
  [`config`], [Your `config.kdl`.],
  [`profile`], [The profile `-p` names, applied over it.],
)

`get` and `show --effective` answer with the last layer that had something to
say; `explain` names it and lists the ones it stood on. `get --written` and
`show` without `--effective` read the file alone, which is what a script
checking whether a key is *authored* wants.

A key is a dotted path, completed by every generated
#link("../cli.typ#completions")[completion script]. A path may name a block of your own:
`content.collections.blog.sort` is `sort` under the collection you called
`blog`.

#callout(kind: "note")[
  `set` edits the KDL as a document: comments, blank lines and the order of
  blocks come back out unchanged. Nothing is written that would not
  parse, so a value the key refuses leaves the file alone.
]

`check --compact` reports each fault as one `file:line:column: message` line,
which is what a tool reading the output expects; it is what
#link("../../build/linting.typ")[`check { snippets }`] runs to check a `kdl` fence.
