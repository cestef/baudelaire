#let frontmatter = (
  order: 12,
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
#build.profile           // the active --profile name, when one is set
#build.git.hash          // full commit SHA (slice it for a short form)
#build.git.rev           // revision number (commit count), when available
#build.git.dirty         // uncommitted changes?
#build.site.title        // your configured site title
#build.site.url          // the configured base url
#build.site.lang         // the configured language
#build.site.author       // the configured author
#build.client.analytics  // any constant from your `client { }` block
```

Inside a git repository `git.hash` and `git.dirty` are always set; `git.rev`,
`git.branch`, `git.tag`, and `git.committed` appear only when git can supply them
(so guard optional reads). `profile` is present only when a `--profile` is
active. `site` mirrors your config (`title`, `url`, `lang`, `author`), and
`client` mirrors the `client { }` block, the same constants the
#link("../assets/js-modules.typ")[#raw("baudelaire:config")] module exposes to
client JavaScript. The footer on this page is built from these values. The Typst
compiler version comes from Typst itself, through the built-in `sys.version`.

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

The same `typst` block also carries `features`, opting into extra experimental
Typst features. HTML export is always enabled for you, so this list is purely
additive, you only name the extras you want:

```kdl
typst {
  features "+a11y-extras"
}
```

#callout(kind: "note")[
  Per-value tracking applies to the `sys.inputs.baudelaire.*` build facts: a new
  commit rebuilds only the pages that read the commit, a new day only the pages
  that show the date, so version stamps never go stale. Your own `typst.inputs`
  values fold into the config fingerprint instead, so changing one rebuilds the
  whole site.
]
