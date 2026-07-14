#let frontmatter = (
  order: 11,
  title: "Build metadata",
  tags: ("feature",),
)
#import "/templates/theme.typ": callout

Every page can read facts about the build from `sys.inputs.baudelaire`: the
version, the date, the mode, and, inside a git repository, the commit. Use it for
a version stamp, a "last built" line, or a commit link in your footer.

```typ
#let build = sys.inputs.baudelaire
#build.version           // "0.1.0"  (Baudelaire's version)
#build.date              // "2026-07-09"
#build.mode              // "build" | "serve" | "check"
#build.git.hash          // full commit SHA (slice it for a short form)
#build.git.rev           // revision number (commit count), when available
#build.git.dirty         // uncommitted changes?
#build.site.title        // your configured site title
```

Inside a git repository `git.hash` and `git.dirty` are always set; `git.rev`,
`git.branch`, `git.tag`, and `git.committed` appear only when git can supply them
(so guard optional reads). The footer on this page is built from these values. The Typst compiler
version comes from Typst itself, through the built-in `sys.version`.

== Your own inputs

Anything under a `typst.inputs` block is exposed at `sys.inputs`, so you can
thread site-wide constants into your pages:

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

#callout(kind: "tip")[
  Build metadata folds into the cache key, so a new commit or a new day rebuilds
  the pages that display it. Version stamps never go stale.
]
