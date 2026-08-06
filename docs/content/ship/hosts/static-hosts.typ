#let frontmatter = (
  title: "Static hosts",
  order: 8,
)
#import "/templates/theme.typ": callout

`public/` is a plain folder of files. Any host that serves a directory serves
it. On a host that builds for you, two settings are the whole configuration:

#table(
  columns: 2,
  align: (left, left),
  table.header([Setting], [Value]),
  [Build command],
  [`curl -fsSL https://baudelaire.cstef.dev/install.sh | sh && ~/.local/bin/baudelaire build`],

  [Publish directory], [`public`],
)

None of these platforms ships a Rust toolchain, so the build command installs
the prebuilt binary first. It lands in `~/.local/bin`, which is not on the build
image's `PATH`, hence the full path.

== Netlify

`netlify.toml` at the repository root, so the settings live with the site:

```toml
[build]
  command = "curl -fsSL https://baudelaire.cstef.dev/install.sh | sh && ~/.local/bin/baudelaire build"
  publish = "public"
```

== Cloudflare Pages

Same two values in the project's build settings. To publish from your own CI
instead, build and hand the folder to Wrangler:

```sh
baudelaire build
wrangler pages deploy public --project-name=my-site --branch=main
```

== Vercel

Set *Build Command* to the line above and *Output Directory* to `public`, either
in the project settings or as `buildCommand` and `outputDirectory` in
`vercel.json`. Vercel reads neither rule file below; its redirects and headers
go in `vercel.json`.

== Your own web server

Copy `public/` where the server roots. Or skip the copy and let baudelaire
reconcile the directory over SSH: see #link("ssh.typ")[SSH & SFTP].

#callout(kind: "tip")[
  With clean URLs a page lives at `posts/hello/index.html`. Most servers serve
  `index.html` for a directory already. If yours does not, turn on its pretty
  URL or index rewrite option.
]

== Rule files

Netlify and Cloudflare Pages both read two files from the publish directory, and
baudelaire writes either on request:

```kdl
caching { }

generate {
  headers #true
  redirects #true
}
```

#table(
  columns: 3,
  align: (left, left, left),
  table.header([Key], [Type], [Does]),
  [`headers`],
  [flag or block],
  [Write `_headers` from the caching and CSP policies, plus any rule the block adds.],

  [`redirects`],
  [flag],
  [Write `_redirects` from each page's declared aliases.],
)

=== `_headers`

The file states the `caching` policy once, so a site that also uploads to a
#link("s3.typ")[bucket] cannot end up with two different answers. Fingerprinted
assets get the immutable value, everything else the revalidating one:

```kdl
caching {
  immutable "public, max-age=604800, immutable"
  default   "public, max-age=300"
}
```

Both keys default to a sensible policy, so a bare `caching { }` is a complete
declaration. The asset rule is only written when
#link("../../build/assets.typ")[`assets { fingerprint }`] is on: without it an
asset keeps its authored name across builds and is exactly as mutable as a page.

A configured #link("../security.typ")[Content-Security-Policy] is written into the
same file.

Anything else the site wants to send is a rule of its own: a path pattern, and
the headers it adds.

```kdl
generate {
  headers {
    "/private/*" {
      X-Robots-Tag "noindex"
    }
    "/*" {
      X-Frame-Options "DENY"
    }
  }
}
```

These come first in the file, before the two derived rules, which end in a
catch-all. Patterns are written relative to the site, so a site under a
#link("../../configure/overview.typ")[base path] gets that path prefixed for it.
A block replaces the whole list, like every other list in the config, and the
flag still stands in front of it: `headers #false { .. }` writes nothing.

#callout(kind: "warn")[
  `generate { headers }` on its own writes nothing. It needs a `caching { }`
  block, a CSP, or a rule of its own to have something to say, and an empty rule
  file means what the host already assumed.
]

=== `_redirects`

Each `redirect` entry in a page's frontmatter becomes a real `301` line instead
of an HTML stub:

```text
/old-path /posts/hello/ 301
```

It *replaces* the #link("../../write/collections/redirects.typ")[stubs] rather than joining
them. Both hosts serve a static file in preference to a redirect rule, so a stub
left at the old path would win and the rule would never fire.

#callout(kind: "note")[
  Leave both off for a host that reads neither, GitHub Pages included. The stubs
  work anywhere, which is why they are the default, and cache headers are then
  your host's to set.
]
