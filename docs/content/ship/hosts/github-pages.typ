#let frontmatter = (
  title: "GitHub Pages",
  order: 5,
)
#import "/templates/theme.typ": callout

Set *Settings > Pages > Source* to *GitHub Actions*, then commit this workflow.
It installs the prebuilt binary, caches the incremental build state, and hands
`public/` to Pages.

```yaml
name: pages

on:
  push:
    branches: [main]
  workflow_dispatch:

permissions:
  contents: read
  pages: write
  id-token: write

concurrency:
  group: pages
  cancel-in-progress: true

jobs:
  deploy:
    runs-on: ubuntu-latest
    environment:
      name: github-pages
      url: ${{ steps.deploy.outputs.page_url }}
    steps:
      - uses: actions/checkout@v7

      - name: install baudelaire
        run: |
          curl -fsSL https://baudelaire.cstef.dev/install.sh | sh
          echo "$HOME/.local/bin" >> "$GITHUB_PATH"

      - name: restore build cache
        uses: actions/cache@v6
        with:
          path: .baudelaire
          key: baudelaire-${{ hashFiles('content/**', 'assets/**', 'config.kdl') }}
          restore-keys: baudelaire-

      - run: baudelaire build

      - uses: actions/upload-pages-artifact@v5
        with:
          path: public

      - id: deploy
        uses: actions/deploy-pages@v5
```

The `permissions`, `environment` and `concurrency` blocks are all required for
`deploy-pages` to work. Drop any of them and the deploy step fails with a
permissions error.

== Project pages

A repository site is served under `/<repo>/`, not at the domain root. Put that
path in `url` and every root-absolute link, feed and sitemap entry picks it up:

```kdl
url "https://USER.github.io/REPO"
```

Nothing else changes. The on-disk layout of `public/` stays the same.

#callout(kind: "tip")[
  For a preview deploy of the same site elsewhere, override it for one run with
  `baudelaire build --base-url "https://preview.example.com"`.
]

== The install step

The #link("../../start/install.typ")[installer] drops the binary in
`~/.local/bin`, which is why the second line appends it to `GITHUB_PATH`. It
downloads a prebuilt release for `x86_64` and `aarch64` Linux, so a GitHub-hosted
runner is always covered. Pin a release with `VERSION`:

```yaml
      - name: install baudelaire
        run: |
          curl -fsSL https://baudelaire.cstef.dev/install.sh | VERSION=v0.0.9 sh
          echo "$HOME/.local/bin" >> "$GITHUB_PATH"
```

#callout(kind: "note")[
  `cargo install baudelaire` works too, but a cold compile is minutes against
  the installer's seconds. Reach for it only on a runner architecture the
  release matrix does not cover.
]

== Git metadata

`actions/checkout` clones one commit by default. That is enough for
`sys.inputs.baudelaire.git.hash`, but `git.rev` (the commit count) and
`git.tag` need history and tags:

```yaml
      - uses: actions/checkout@v7
        with:
          fetch-depth: 0
```

See #link("../../lookup/context.typ")[Build metadata] for what the build exposes.

== Rule files

GitHub Pages reads neither `_headers` nor `_redirects`, so leave
`generate { headers }` and `generate { redirects }` off. Redirects fall back to
HTML stubs, which work anywhere, and cache headers are GitHub's to set. Both
files are for #link("static-hosts.typ")[Netlify and Cloudflare Pages].

== Pushing instead

Nothing forces you through the Pages actions. `baudelaire deploy` from the same
workflow uploads to a #link("s3.typ")[bucket] or an #link("ssh.typ")[SSH host]
instead:

```yaml
      - run: baudelaire deploy --yes
        env:
          AWS_ACCESS_KEY_ID: ${{ secrets.AWS_ACCESS_KEY_ID }}
          AWS_SECRET_ACCESS_KEY: ${{ secrets.AWS_SECRET_ACCESS_KEY }}
```

`deploy` builds first, so it replaces the `baudelaire build` step rather than
following it, and the two Pages steps and the `pages`/`id-token` permissions go
with it. `--yes` is not optional here: off a terminal, an unconfirmed deploy is
an error.
