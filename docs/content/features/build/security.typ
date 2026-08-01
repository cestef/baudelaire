#let frontmatter = (
  order: 27,
  title: "Integrity and CSP",
  tags: ("feature", "security"),
)
#import "/templates/theme.typ": callout

Two of the things a static site is told to ship are exactly the two nobody can
keep up to date by hand: an `integrity` attribute on every file a page loads,
and a `Content-Security-Policy` naming every inline script it runs. Both go
stale the moment a template changes. Baudelaire derives both from the pages it
just built, so neither can describe a site it did not produce.

== Subresource integrity

```kdl
assets { fingerprint #true }
security { sri }
```

Every `<script src>` and `<link rel="stylesheet">` pointing at a file *this
build wrote* gets the SHA-384 of that file's bytes:

```html
<link rel="stylesheet" href="/assets/app.7c1d93fc.css"
      integrity="sha384-PuNg60FoG6nUCJ2IUl821GnDVQXMKrFG53BmN6qoeQYV/eMOkWjCKMiax8JHNFX3">
```

A reference to somebody else's host is left alone: the digest would be a guess
about bytes this build never saw, and a wrong one blocks the file rather than
protecting it. An `integrity` you wrote yourself is also left alone; pinning a
specific digest is the one thing the attribute is for.

#callout(kind: "warning")[
  `sri` needs `assets { fingerprint }` and says so if it does not have it. An
  attribute pins a digest to a URL, so the URL has to name one exact file: with
  unhashed names the file behind `/assets/app.css` changes while the pages
  naming it stay cached, and every one of them then blocks the stylesheet it
  asked for.
]

== Content security policy

```kdl
generate { headers #true }
security { csp { } }
```

The block's presence generates a policy into `_headers`, the rule file Netlify
and Cloudflare Pages read out of the publish directory. A silent block is
already a real policy, `default-src 'self'`, plus the half you could not have
written:

```
/*
  Content-Security-Policy: default-src 'self'; script-src 'self' 'sha256-Cihokc…'
```

Each directive is a key, and its value is a CSP source list, taken verbatim:

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

An unset directive is not written at all, so it falls back to `default-src`.
`img "'self' data:"` is the one most sites need, since `html { embed }` inlines
assets as `data:` URIs.

=== The inline half

Every inline `<script>`, `<style>` and `style=""` attribute the build produced
is digested and named in `script-src` / `style-src`, which is what lets a policy
forbid inline code in general and still run the blocks your own pages carry: the
speculation rules, the SPA runtime, a theme's inline style. The digests are
unioned across the site and deduplicated, so a block emitted on every page is
one entry.

The attributes matter more than they sound: typst resolves an element's CSS
properties into a `style` attribute, and so do the image show rule and the
syntax highlighter, so a typical page carries several nobody wrote. Allowing one
by digest takes `'unsafe-hashes'` beside it, which is added only when the site
has such an attribute. Despite the name it is still an allowlist of exact
strings this build produced; the alternative is `'unsafe-inline'`, which allows
every inline style there could be.

The fallback comes along with them. A stated directive *replaces* `default-src`
rather than extending it, so `script-src` is written as the configured value (or
`default-src`'s) followed by the digests, and never as the digests alone, which
would have blocked every script file the page loads.

#callout(kind: "note")[
  Hashing turns `html { pretty }` off. A browser digests the bytes between
  `<script>` and `</script>` exactly as served, and the pretty printer re-indents
  a script body on its way out, after the point the digest was taken. A policy
  built that way names a body nobody was served. Set
  `security { csp { hashes #false } }` to keep the indentation and lose the
  digests.
]

=== Rolling one out

```kdl
security { csp { enforce #false } }
```

emits `Content-Security-Policy-Report-Only` instead: the same policy, reported
and not enforced, which is how you find out what a real one would have broken.

=== What it does not cover

Two things the policy cannot name, both of which will block if you serve them
under it:

/ `html { embed }`: an embedded script or stylesheet becomes a `data:` URI, and
  it is *loaded*, not inlined, so no digest applies. Add `data:` to the
  directive yourself (`script "'self' data:"`) if you want both, knowing that a
  `data:` source is a wide allowance.
/ `navigation { standalone }`: the single-file export's router is written after
  the pages are, by the processor that assembles it, so its script is not among
  the digests. Opening the file works; serving it under this policy does not.

== One policy, one header

The policy covers `/*` rather than each page separately. `_headers` applies
every matching rule, so a page carrying its own policy would be served that one
*and* the catch-all, and a browser enforces the intersection: the catch-all,
which does not name that page's inline blocks, would block them. The looseness
traded away is small, since a digest allows one exact body and nothing else, and
every one of those bodies is the build's own.
