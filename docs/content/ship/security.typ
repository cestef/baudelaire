#let frontmatter = (
  title: "Integrity & CSP",
  order: 3,
)
#import "/templates/theme.typ": callout

An `integrity` attribute on every file a page loads, and a
`Content-Security-Policy` naming every inline block it runs. Both derived from
the pages this build just produced.

```kdl
assets {
  fingerprint #true
}
generate {
  headers #true
}

security {
  sri #true
  csp {
    img "'self' data:"
  }
}
```

== Subresource integrity

```kdl
assets {
  fingerprint #true
}

security {
  sri #true
}
```

Every `script src` and `link rel="stylesheet"` pointing at a file *this build
wrote* gets the SHA-384 of that file's bytes:

```text
<link rel="stylesheet" href="/assets/app.7c1d93fc.css"
      integrity="sha384-PuNg60FoG6nUCJ2IUl821GnDVQXMKrFG53BmN6qoeQYV/eMOkWjCKMiax8JHNFX3">
```

Two things are left alone: a reference to someone else's host (the digest would
be a guess about bytes this build never saw, and a wrong one blocks the file
instead of protecting it), and an `integrity` you wrote yourself.

#callout(kind: "warn")[
  `sri` needs `assets { fingerprint }` and tells you so if it doesn't have it.
  With unhashed names the file behind `/assets/app.css` changes while the pages
  naming it stay cached, and every one of them then blocks the stylesheet it
  asked for.
]

== Content security policy

```kdl
generate {
  headers #true
}

security {
  csp { }
}
```

The block's presence generates a policy into `_headers`, the rule file Netlify
and Cloudflare Pages read out of the publish directory. See
#link("hosts/static-hosts.typ")[static hosts]. A silent block is already a real
policy:

```text
/*
  Content-Security-Policy: default-src 'self'; script-src 'self' 'sha256-Cihokc...'
```

Without `generate { headers }` the block writes nothing and the build says so.

=== Writing the directives

Each directive is a key, and its value is a CSP source list taken verbatim:

```kdl
security {
  csp {
    default "'self'"
    script "'self'"
    style "'self'"
    img "'self' data:"
    font "'self'"
    connect "'self'"
    frame "'none'"
    object "'none'"
    base "'self'"
    form "'self'"
    report "https://csp.example.com/report"
  }
}
```

#table(
  columns: 3,
  align: (left, left, left),
  table.header([Key], [Directive], [Default]),
  [`default`], [`default-src`], [`'self'`],
  [`script`], [`script-src`], [--],
  [`style`], [`style-src`], [--],
  [`img`], [`img-src`], [--],
  [`font`], [`font-src`], [--],
  [`connect`], [`connect-src`], [--],
  [`frame`], [`frame-src`], [--],
  [`object`], [`object-src`], [--],
  [`base`], [`base-uri`], [--],
  [`form`], [`form-action`], [--],
  [`report`], [`report-uri`], [--],
)

An unset directive isn't written at all, so it falls back to `default-src`.
`img "'self' data:"` is the one most sites need, since
#link("../build/assets.typ")[`html { embed }`] inlines assets as `data:` URIs.

Two more keys control the policy rather than a directive:

#table(
  columns: 4,
  align: (left, left, left, left),
  table.header([Key], [Type], [Default], [Does]),
  [`enforce`],
  [bool],
  [`#true`],
  [Off emits `Content-Security-Policy-Report-Only` instead.],

  [`hashes`],
  [bool],
  [`#true`],
  [Off drops the inline digests and keeps `html { pretty }`.],
)

=== The inline half

Every inline `script`, `style` and `style=""` attribute the build produced is
digested and named in `script-src` / `style-src`. That's what lets a policy
forbid inline code in general and still run the blocks your own pages carry: the
speculation rules, the SPA runtime, a theme's inline style. Digests are unioned
across the site and deduplicated, so a block emitted on every page is one entry.

The attributes matter more than they sound. Typst resolves an element's CSS
properties into a `style` attribute, and so do the image rule and the syntax
highlighter, so a typical page carries several nobody wrote. Allowing one by
digest takes `'unsafe-hashes'` beside it, which the build adds only when the
site has such an attribute. It's still an allowlist of exact strings this build
produced; the alternative is `'unsafe-inline'`, which allows every inline style
there could be.

A stated directive *replaces* `default-src` rather than extending it, so
`script-src` is written as the configured value (or `default-src`'s) followed by
the digests, never as the digests alone.

#callout(kind: "warn")[
  Hashing turns `html { pretty }` off. The pretty printer re-indents a script
  body after the digest was taken, so the policy would name a body nobody was
  served. Set `security { csp { hashes #false } }` to keep the indentation and
  lose the digests.
]

=== Rolling one out

```kdl
security {
  csp {
    enforce #false
  }
}
```

Emits `Content-Security-Policy-Report-Only`: the same policy, reported and not
enforced. Deploy that, set `report` to a collector, read what fires, then flip
`enforce` back on.

=== What it doesn't cover

Two things the policy can't name, both of which will block if you serve them
under it.

*`html { embed }`.* An embedded script or stylesheet becomes a `data:` URI, and
it's *loaded*, not inlined, so no digest applies. Add `data:` to the directive
yourself (`script "'self' data:"`), knowing that a `data:` source is a wide
allowance.

*`navigation { standalone }`.* The
#link("navigating.typ")[single-file export]'s router is written after the pages
are, so its script isn't among the digests. Opening the file works; serving it
under this policy doesn't.

=== One policy, one header

The policy covers `/*` rather than each page separately. `_headers` applies
every matching rule, so a page carrying its own policy would be served that one
*and* the catch-all, and a browser enforces the intersection. The catch-all
doesn't name that page's inline blocks, so it would block them.
