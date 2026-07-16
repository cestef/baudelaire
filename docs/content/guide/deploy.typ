#let frontmatter = (
  title: "Deploying",
  order: 7,
)
#import "/templates/theme.typ": callout

`baudelaire build` writes a plain folder of static files to `public/` — the
`dist` directory from #link("config.typ")[config], overridable with `--out`.
There is no server to run. Host it anywhere that serves files.

== Any static host

Upload `public/` to Netlify, Vercel, Cloudflare Pages, GitHub Pages, S3, or your
own nginx. The typical settings:

/ Build command: #raw("baudelaire build")
/ Publish directory: #raw("public")

Hosts that build for you (Netlify, Vercel, Cloudflare Pages) have no Rust
toolchain, so install the prebuilt binary first in the build command:

```sh
curl -fsSL https://baudelaire.cstef.dev/install.sh | sh && ~/.local/bin/baudelaire build
```

#callout(kind: "tip")[
  With clean URLs on, a page lives at `posts/hello/index.html`. Most hosts serve
  `index.html` for a directory automatically; if yours doesn't, point its
  "pretty URLs" or rewrite option at `index.html`.
]

== GitHub Pages

Set #emph[Settings → Pages → Source] to #emph["GitHub Actions"], then commit this
workflow. It installs the prebuilt binary (no compile), caches the incremental
build state, and hands `public/` to Pages. The `permissions`, `environment`, and
`concurrency` blocks are all required for `deploy-pages` to work.

```yaml
name: deploy
on:
  push: { branches: [main] }

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
      - name: Install baudelaire
        run: |
          curl -fsSL https://baudelaire.cstef.dev/install.sh | sh
          echo "$HOME/.local/bin" >> "$GITHUB_PATH"
      - uses: actions/cache@v6
        with:
          path: .baudelaire
          key: baudelaire-${{ hashFiles('content/**', 'assets/**', 'config.kdl') }}
          restore-keys: baudelaire-
      - run: baudelaire build
      - uses: actions/upload-pages-artifact@v5
        with: { path: public }
      - id: deploy
        uses: actions/deploy-pages@v5
```

#callout(kind: "note")[
  Prefer `cargo install baudelaire` in CI only if you already cache the Cargo
  build — a cold compile is minutes, the prebuilt binary is seconds. On
  non-`x86_64`/`aarch64` runners the installer has no binary, so fall back to
  `cargo install`.
]

== GitLab Pages

GitLab Pages serves the `public/` artifact of a job named `pages` — which is
exactly what `baudelaire build` produces. The installer picks the glibc or musl
binary to match the image, so Alpine works too:

```yaml
pages:
  image: alpine:latest
  script:
    - apk add --no-cache curl
    - curl -fsSL https://baudelaire.cstef.dev/install.sh | sh
    - ~/.local/bin/baudelaire build
  artifacts:
    paths: [public]
  cache:
    key: baudelaire
    paths: [.baudelaire]
  rules:
    - if: $CI_COMMIT_BRANCH == $CI_DEFAULT_BRANCH
```

== Forgejo / Gitea Actions

The GitHub workflow above runs almost verbatim on a Forgejo or Gitea runner —
keep the `build` and `upload`/`deploy` steps, and swap the Pages actions for
whatever your instance provides (many just publish the `public/` artifact).

== Built-in publish

For hosts baudelaire pushes to directly — no CI — `baudelaire publish` sends the
built site to every destination in the config. See the
#link("cli.typ")[CLI reference].

== Preview builds

Point the canonical URL at the preview host so feeds and the sitemap use the
right absolute links:

```sh
baudelaire build --base-url "https://preview.example.com"
```

Because builds are #link("../features/build/incremental.typ")[incremental], caching
the `.baudelaire/` directory between CI runs makes rebuilds near-instant — as the
workflows above do.
