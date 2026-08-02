#let frontmatter = (
  title: "Forgejo & Gitea",
  order: 7,
)
#import "/templates/theme.typ": callout

Forgejo and Gitea run the same workflow syntax as
#link("github-pages.typ")[GitHub Actions], so the build half is unchanged. What
differs is where actions are fetched from, what `runs-on` names, and the fact
that there is no Pages deploy action to hand `public/` to.

Put this in `.forgejo/workflows/deploy.yml` (Gitea: `.gitea/workflows/`). It
builds and uploads with `baudelaire deploy`:

```yaml
name: deploy

on:
  push:
    branches: [main]
  workflow_dispatch:

jobs:
  deploy:
    runs-on: docker
    steps:
      - uses: https://code.forgejo.org/actions/checkout@v4

      - name: install baudelaire
        run: curl -fsSL https://baudelaire.cstef.dev/install.sh | sh

      - name: restore build cache
        uses: https://code.forgejo.org/actions/cache@v4
        with:
          path: .baudelaire
          key: baudelaire-${{ hashFiles('content/**', 'assets/**', 'config.kdl') }}
          restore-keys: baudelaire-

      - name: build and upload
        run: ~/.local/bin/baudelaire deploy --yes
        env:
          AWS_ACCESS_KEY_ID: ${{ secrets.AWS_ACCESS_KEY_ID }}
          AWS_SECRET_ACCESS_KEY: ${{ secrets.AWS_SECRET_ACCESS_KEY }}
```

`deploy` builds the site itself, so there is no separate build step. Point it at
a #link("s3.typ")[bucket] or an #link("ssh.typ")[SSH host] in `config.kdl`.

== Four things that differ from GitHub

#table(
  columns: 2,
  align: (left, left),
  table.header([On GitHub], [Here]),
  [`uses: actions/checkout@v7`],
  [Write the full URL. A bare name resolves against the instance's configured
  action host, which is not github.com.],

  [`runs-on: ubuntu-latest`],
  [Whatever label your runner registered. `docker` is the usual one, and it is
  Codeberg's.],

  [`$GITHUB_PATH` for the install dir],
  [Call `~/.local/bin/baudelaire` outright. One less runner behavior to depend
  on.],

  [`deploy-pages` publishes the artifact],
  [No Pages action exists. Push the files yourself, or use `baudelaire deploy`.],
)

The installer needs `curl` (or `wget`) and `tar`. The `node:20-bookworm` image
most runners default to has both. On a barer image, install them in the step
above: `apt-get update && apt-get install -y curl ca-certificates` on
Debian-family, `apk add --no-cache curl ca-certificates` on Alpine.

#callout(kind: "note")[
  Actions are off by default on most instances. Enable them for the repository
  and register a runner before any of this runs.
]

== Codeberg Pages

Codeberg serves the `pages` branch of a repository at
`https://USER.codeberg.page/REPO`. Build, then force-push `public/` as that
branch:

```yaml
      - name: build
        run: ~/.local/bin/baudelaire build

      - name: publish to the pages branch
        run: |
          cd public
          git init -q -b pages
          git add -A
          git -c user.name=ci -c user.email=ci@example.com \
              commit -qm "deploy ${{ github.sha }}"
          git push -f \
            "https://${{ secrets.PAGES_TOKEN }}@codeberg.org/${{ github.repository }}.git" \
            pages
```

`PAGES_TOKEN` is a repository secret holding an access token with write
permission. The branch is rebuilt from scratch every run, which is what the
force push is for: the history of a generated directory is not worth keeping.

A site there lives under `/REPO`, so state that in config or the links will
point at the domain root:

```kdl
url "https://USER.codeberg.page/REPO"
```

A repository actually named `pages` is served at `https://USER.codeberg.page`
with no path prefix, in which case leave `url` at the bare domain.

#callout(kind: "tip")[
  Codeberg Pages reads a `.domains` file from the published branch for a custom
  domain. Put it in `paths { static }` and the build copies it through
  untouched.
]
