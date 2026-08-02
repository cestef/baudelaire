#let frontmatter = (
  title: "Build metadata",
  order: 4,
)
#import "/templates/theme.typ": callout

What a page can learn about the build that produced it, at
`sys.inputs.baudelaire`. Use it for a version stamp, a "last built" line, or a
commit link in the footer.

```typ
#let build = sys.inputs.baudelaire

#build.version           // baudelaire's version
#build.date              // "2026-08-02"
#build.mode              // "build" | "serve" | "check"
#build.git.hash          // full commit SHA
#build.site.title        // your configured site title
#build.client.analytics  // any constant from your `client { }` block
```

== What it carries

#table(
  columns: 3,
  align: (left, left, left),
  table.header([Key], [Type], [Is]),
  [`version`], [str], [Baudelaire's own version.],
  [`date`], [str], [The build date, `YYYY-MM-DD`.],
  [`mode`], [str], [`build`, `serve`, or `check`.],
  [`profile`], [str], [The active #link("../configure/profiles.typ")[`--profile`]. Absent when none is set.],
  [`git`], [dict], [Repository state. Absent outside a git repository.],
  [`site`], [dict], [`title`, `url`, `lang`, `author`, `languages`, from the config.],
  [`client`], [dict], [The `client { }` block, verbatim.],
)

`site` mirrors the config, and unset values read as `none`. It is the same data
`@baudelaire/site` binds by name, which is the nicer way to reach it: see
#link("typst-modules.typ")[Typst modules]. `client` is the same set of constants
the #link("js-modules.typ")[`baudelaire:config`] module hands your client
JavaScript, so Typst and the browser read one source.

Typst's own version comes from typst, at `sys.version`.

== git

#table(
  columns: 3,
  align: (left, left, left),
  table.header([Key], [Type], [Is]),
  [`hash`], [str], [The full commit SHA. Slice it yourself for a short form.],
  [`dirty`], [bool], [Whether the tree had uncommitted changes.],
  [`rev`], [str], [Commits reachable from HEAD.],
  [`branch`], [str], [The current branch.],
  [`tag`], [str], [The nearest reachable tag.],
  [`committed`], [str], [The commit's date, ISO 8601.],
)

`hash` and `dirty` are always set inside a repository. The rest appear only when
git can supply them, so guard those reads:

```typ
#let git = sys.inputs.baudelaire.at("git", default: none)
#if git != none [
  Built from #raw(git.hash.slice(0, 7))#if git.dirty [ (dirty)]
]
```

== Your own inputs

Anything under `typst { inputs }` lands at `sys.inputs`, for threading site-wide
constants into your pages:

```kdl
typst {
  inputs {
    environment "production"
  }
}
```

```typ
#sys.inputs.environment  // "production"
```

Values arrive as strings, whatever they look like in KDL. `baudelaire` is
reserved for the block above. For constants that also need to reach client
JavaScript, use `client { }` instead.

#callout(kind: "note")[
  The `sys.inputs.baudelaire.*` facts are tracked per value: a new commit
  rebuilds only the pages that read the commit, a new day only the ones that show
  the date. Your own `typst { inputs }` values fold into the config fingerprint
  instead, so changing one rebuilds the whole site.
]
