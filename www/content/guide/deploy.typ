#frontmatter((
  title: "Deploying",
  order: 5,
  tags: ("guide",),
))
#import "/templates/theme.typ": callout

`baudelaire build` writes a plain folder of static files to `public/`. There is
no server to run. Host it anywhere that serves files.

== Any static host

Upload `public/` to Netlify, Vercel, Cloudflare Pages, GitHub Pages, S3, or your
own nginx. The typical settings:

/ Build command: #raw("baudelaire build")
/ Publish directory: #raw("public")

#callout(kind: "tip")[
  With clean URLs on, a page lives at `posts/hello/index.html`. Most hosts serve
  `index.html` for a directory automatically; if yours doesn't, point its
  "pretty URLs" or rewrite option at `index.html`.
]

== GitHub Pages

```yaml
name: deploy
on:
  push: { branches: [main] }
jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - run: cargo install baudelaire
      - run: baudelaire build
      - uses: actions/upload-pages-artifact@v3
        with: { path: public }
      - uses: actions/deploy-pages@v4
```

== Preview builds

Point the canonical URL at the preview host so feeds and the sitemap use the
right absolute links:

```sh
baudelaire build --base-url "https://preview.example.com"
```

Because builds are #link("../features/incremental.typ")[incremental], caching
the `.baudelaire/` directory between CI runs makes rebuilds near-instant.
