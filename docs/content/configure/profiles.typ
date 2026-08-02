#let frontmatter = (
  title: "Profiles & environment",
  order: 2,
)
#import "/templates/theme.typ": callout

Named overlays on top of `config.kdl`, and `${VAR}` in any string value.

```kdl
profiles {
  dev {
    content {
      draft { build #true }
    }
    assets {
      minify #false
      bundle #false
      fingerprint #false
    }
  }
}
```

```sh
baudelaire build --profile dev
```

Drafts are built and the asset pipeline is skipped, so rebuilds are faster. `-p`
is the short spelling.

== Applying one

`--profile` is a global flag, so it works on every command that reads the config:
`build`, `serve`, `check`, `deploy`, `announce`. `serve` keeps the profile across
reloads, so a config edit mid-session does not silently drop back to the base.

A profile accepts every key the top level accepts. Two things fail:

- An unknown name. The error lists the profiles you did configure, with a
  nearest match.
- A `profiles` block inside a profile. Overlays do not nest.

== What an overlay replaces

#table(
  columns: 2,
  align: (left, left),
  table.header([In the overlay], [What happens]),
  [A block (`html`, `assets`, `content { draft }`)],
  [Filled in place. Keys the overlay does not name keep the values they had.],

  [A scalar (`url`, `prune`, `serve { port }`)],
  [Replaced.],

  [A list (`html { footnotes }`, `serve { include }`, `generate { feed { formats } }`)],
  [Replaced wholesale. There is no appending.],

  [A named set (`content { collections }`, `content { taxonomies }`, `languages`)],
  [Replaced wholesale. The base set is dropped, not merged.],
)

Filling in place is what makes a one-key overlay useful:

```kdl
assets {
  minify #true
  bundle #true
  fingerprint #true
}

profiles {
  preview {
    assets { minify #false }
  }
}
```

Under `--profile preview`, minification is off and bundling and fingerprinting
are still on. The same holds at any depth: a profile touching
`content { draft { build } }` leaves `content { index }` and every collection
alone.

#callout(kind: "warn")[
  A profile that declares `collections` replaces the whole set, so any collection
  it does not restate stops existing. Restate all of them, or override something
  else.
]

== Environment variables

Any string value can read the environment with `${VAR}`, with an optional
`${VAR:-default}` fallback:

```kdl
url "${SITE_URL:-http://localhost:1821}"

typst {
  inputs {
    environment "${DEPLOY_ENV:-development}"
  }
}

profiles {
  prod {
    url "https://example.com"
    deploy {
      s3 {
        bucket "${S3_BUCKET}"
        endpoint "${S3_ENDPOINT}"
        region "auto"
      }
    }
  }
}
```

An unset variable with no default is a hard error, not an empty string, so a
missing secret fails the build instead of shipping a blank value. Give it a
`${VAR:-default}` when an empty or fallback result is acceptable.

#callout(kind: "note")[
  Expansion happens as the config is read, so it applies to every string: paths,
  hook commands, `typst { inputs }`, deploy targets. A string inside a profile is
  read only when that profile is applied, so `${S3_BUCKET}` above is nobody's
  problem until `--profile prod`.
]

A `${` with no closing brace is left alone, so a literal one in a hook command
survives.

== Profiles or flags

`--drafts`, `--base-url`, `--out` and `--no-cache` do the common one-off
overrides without a profile, and they apply on top of whichever profile is
active. Reach for a profile when the change is more than one setting, or when
you want it committed. See the #link("../lookup/cli.typ")[CLI reference].
