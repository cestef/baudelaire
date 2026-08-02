#let frontmatter = (
  title: "Deploying",
  order: 1,
)
#import "/templates/theme.typ": callout

`baudelaire build` writes a plain folder of files to `public/`. Hand that folder
to any host, or let `baudelaire deploy` push it for you.

```sh
baudelaire deploy --dry-run   # print the plan, upload nothing
baudelaire deploy             # confirm, then upload
baudelaire deploy --yes       # no prompt, for CI
```

== What a deploy does

It builds the site first, so a run always reflects the current sources. Then it
reconciles the destination with `public/`: uploads what is new or changed, skips
what already matches, and deletes what the build no longer produces.

A dry run stops after printing that plan, one line per destination: how many
files would upload, how many would be deleted, how many are already current.

Change detection compares content digests, and both backends read those digests
from the remote itself. There is no local state to keep or lose, so re-deploying
an unchanged site uploads nothing.

== Pick a destination

Two destinations are built in. Both are configured under `deploy`, and the
presence of the block turns one on:

```kdl
deploy {
  s3 {
    bucket "my-site"
    endpoint "https://ACCOUNT.r2.cloudflarestorage.com"
  }
}
```

#table(
  columns: 3,
  align: (left, left, left),
  table.header([Destination], [Block], [Page]),
  [S3-compatible bucket],
  [`deploy { s3 }`],
  [#link("hosts/s3.typ")[S3-compatible storage]],

  [SSH host, over SFTP],
  [`deploy { ssh }`],
  [#link("hosts/ssh.typ")[SSH & SFTP]],
)

Declare both and each runs in turn, confirmed separately.

#callout(kind: "note")[
  With no `deploy` block at all, `baudelaire deploy` errors instead of exiting 0
  having published nothing.
]

Every other host takes the folder rather than a push:

#table(
  columns: 2,
  align: (left, left),
  table.header([Host], [Page]),
  [GitHub Pages], [#link("hosts/github-pages.typ")[GitHub Pages]],
  [GitLab Pages], [#link("hosts/gitlab-pages.typ")[GitLab Pages]],
  [Forgejo, Gitea, Codeberg], [#link("hosts/forgejo.typ")[Forgejo & Gitea]],
  [Netlify, Vercel, Cloudflare Pages, nginx],
  [#link("hosts/static-hosts.typ")[Static hosts]],
)

== Confirmation

Each destination asks before it writes anything. `-y` / `--yes` grants it up
front.

#callout(kind: "warn")[
  Off a terminal with no `--yes`, the deploy *fails* rather than skipping. A
  publish nobody authorized must not report success.
]

== Credentials

Credentials never live in `config.kdl`. Each backend reads its own environment
variables at deploy time, and `--secret` supplies the one secret it prompts for
(the S3 secret access key, or the SSH password or key passphrase):

```sh
echo "$AWS_SECRET_ACCESS_KEY" | baudelaire deploy --secret -
```

`--secret -` reads one line from stdin, so the value never appears in the
process arguments. A literal `--secret <value>` wins over everything, including
the environment, but it lands in your shell history. Resolution order per
backend is on the #link("hosts/s3.typ")[S3] and #link("hosts/ssh.typ")[SSH] pages.

== Preview builds

Point the canonical URL at the preview host, so feeds, the sitemap and social
cards emit the right absolute links:

```sh
baudelaire build --base-url "https://preview.example.com"
```

The `url` path component is honored too, so a site served under a subdirectory
(`url "https://example.com/docs"`) prefixes every root-absolute link with
`/docs` and leaves the on-disk layout alone. A named
#link("../configure/profiles.typ")[profile] does the same from config:
`baudelaire --profile preview deploy`.

== Flags

#table(
  columns: 2,
  align: (left, left),
  table.header([Flag], [Does]),
  [`--dry-run`], [Report the plan and write nothing.],
  [`-y`, `--yes`], [Skip the confirmation prompt.],
  [`--secret <value>`], [The destination's secret. `-` reads it from stdin.],
  [`--base-url <url>`], [Override the canonical URL for this run.],
  [`--out <dir>`], [Override the output directory.],
  [`--no-cache`], [Ignore the incremental cache and rebuild everything.],
)

`deploy` takes the build flags too, since it builds. See the
#link("../lookup/cli.typ")[CLI reference].

== Builds in CI

Builds are #link("../build/incremental.typ")[incremental], and the state lives
in `.baudelaire/`. Cache that directory between runs and a rebuild only touches
what changed. Every CI recipe on the following pages does it.

== Announcing is not deploying

`baudelaire announce` uploads none of `public/`. It publishes the site's
metadata to an AT Protocol repo. Deploy the files with one of the methods here,
then see #link("announce.typ")[Announcing].
