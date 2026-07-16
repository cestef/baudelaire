#let frontmatter = (
  title: "Overview",
  order: 7,
)
#import "/templates/theme.typ": callout

`baudelaire build` writes a plain folder of static files to `public/`, the
`dist` directory from #link("../config.typ")[config], overridable with `--out`.
There is no server to run.

There are two ways to publish it: the built-in `baudelaire deploy` command, which
uploads the files to a destination you configure, or handing `public/` to any
static host or CI pipeline.

== The `deploy` command

`baudelaire deploy` builds the site first (so a run always reflects the current
sources) then reconciles the configured destination with `public/`: it uploads
new or changed files, skips unchanged ones, and (unless told otherwise) deletes
what the build no longer produces.

```sh
baudelaire deploy --dry-run   # show the plan, write nothing
baudelaire deploy             # confirm, then upload and delete
baudelaire deploy --yes       # skip the confirmation
```

Pick a backend by adding a `deploy` block to `config.kdl`: an
#link("s3.typ")[S3-compatible bucket] or an #link("ssh.typ")[SSH host].
Credentials never live in config: each backend reads them from the environment
(or a prompt) at deploy time.

#callout(kind: "note")[
  No backend configured? `baudelaire deploy` says so rather than doing nothing:
  hand `public/` to a #link("static-hosts.typ")[static host] instead.
]

== Preview builds

Point the canonical URL at the preview host so feeds and the sitemap use the
right absolute links:

```sh
baudelaire build --base-url "https://preview.example.com"
```

Because builds are #link("../../features/build/incremental.typ")[incremental],
caching the `.baudelaire/` directory between CI runs makes rebuilds near-instant.

== Announcing is not deploying

`baudelaire announce` uploads none of `public/`. It broadcasts your site's
metadata to the #link("https://atproto.com")[AT Protocol] via
#link("https://standard.site")[standard.site]. Deploy the files with one of the
methods here, then see #link("../../features/build/announcing.typ")[Announcing].
