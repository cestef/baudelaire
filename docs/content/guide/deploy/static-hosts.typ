#let frontmatter = (
  title: "Static hosts",
  order: 10,
)
#import "/templates/theme.typ": callout

Prefer to let a platform serve the files? `public/` is a plain static folder —
upload it to Netlify, Vercel, Cloudflare Pages, GitHub Pages, or your own nginx.
The typical settings:

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
