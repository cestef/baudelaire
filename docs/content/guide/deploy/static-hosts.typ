#let frontmatter = (
  title: "Static hosts",
  order: 10,
)
#import "/templates/theme.typ": callout

Prefer to let a platform serve the files? `public/` is a plain static folder:
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

== Rule files

Netlify and Cloudflare Pages read two files from the publish directory, and
baudelaire writes either on request:

```kdl
caching { }

generate {
  headers   #true   // _headers: the caching policy above
  redirects #true   // _redirects: real 301s instead of HTML stubs
}
```

`_headers` states the #link("s3.typ")[`caching`] policy once, so a site that
also deploys to a bucket cannot end up with two different answers: assets under
a fingerprinted prefix are immutable, everything else revalidates.

`_redirects` turns each page's `redirect` frontmatter into a server-answered
301. It *replaces* the #link("../../features/content/redirects.typ")[HTML stubs]
rather than joining them, because both hosts serve a static file in preference
to a redirect rule: a stub left at the old path would win and the rule would
never fire.

#callout(kind: "note")[
  Leave both off for a host that reads neither, GitHub Pages included. The stubs
  work anywhere, which is why they are the default, and cache headers are then
  your host's or your web server's to set.
]
