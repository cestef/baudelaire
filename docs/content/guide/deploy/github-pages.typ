#let frontmatter = (
  title: "GitHub Pages",
  order: 11,
)
#import "/templates/theme.typ": callout

Set #emph[Settings -> Pages -> Source] to #emph["GitHub Actions"], then commit this
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
  build: a cold compile is minutes, the prebuilt binary is seconds. On
  non-`x86_64`/`aarch64` runners the installer has no binary, so fall back to
  `cargo install`.
]
