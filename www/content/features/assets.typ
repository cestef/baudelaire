#frontmatter((
  title: "Asset pipeline",
  tags: ("feature", "assets"),
))
#import "/templates/theme.typ": callout

Files in your `assets/` directory can be minified, bundled, and fingerprinted on
the way into the build. Every switch is off by default, so assets are copied
verbatim until you ask for more.

```kdl
output {
  assets {
    minify #true
    bundle #true
    fingerprint #true
  }
}
```

== What each switch does

/ #raw("minify"): CSS is minified with #link("https://lightningcss.dev")[Lightning CSS];
  JavaScript is minified as part of bundling.
/ #raw("bundle"): JavaScript entry points are bundled with
  #link("https://rolldown.rs")[rolldown], with imports resolved and dead code
  shaken out. Files whose name starts with `_` are partials: pulled in through
  imports, never emitted on their own.
/ #raw("fingerprint"): Output filenames get a content hash, turning `style.css`
  into `style.9f3c1a.css`, and every reference in your pages is rewritten to
  match. Now you can cache assets forever and never serve a stale one.

#callout(kind: "note")[
  JavaScript is only processed when `bundle` is on, because a bundler owns the
  whole JS step. CSS minification is independent of it.
]

== How references are rewritten

Write the plain path in your template:

```typ
#html.elem("link", attrs: (rel: "stylesheet", href: "/assets/style.css"))
#html.elem("script", attrs: (type: "module", src: "/assets/main.js"))
```

After the build those point at the hashed filenames. The rewrite works on the
typed HTML tree, never on the serialized string, so it can't corrupt your
markup. This site's stylesheet and script go through exactly this path.

#callout(kind: "tip")[
  Fingerprinted assets fold into the build cache: change one byte of CSS and
  every page that links it is rebuilt with the new URL, automatically.
]
