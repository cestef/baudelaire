#let frontmatter = (
  title: "Linting & budgets",
  order: 6,
)
#import "/templates/theme.typ": callout

Accessibility checks over the built pages, and a byte ceiling on what each one
ships.

```kdl
lint {
  strict
  budget {
    html "50kB"
    css "20kB"
    js 0
  }
}
```

Nothing is linted until the block is there. Its presence turns every rule on, and
`lint #false` turns them all off again, which is how a profile says it. Findings
are warnings: the build still succeeds. `strict` makes a finding fail it instead.

Because pages are post-processed as a typed DOM and every element still carries
the typst span it came from, a finding is reported against the line you wrote,
not a byte offset in generated markup.

== The rules

Each is a flag, on while the block is present, off by name.

#table(
  columns: 3,
  align: (left, left, left),
  table.header([Key], [Type], [Does]),
  [`headings`],
  [flag],
  [Reports a heading that skips a level, `h2` straight to `h4`.],

  [`alt`],
  [flag],
  [Reports an `<img>` with no `alt` attribute at all.],

  [`ids`],
  [flag],
  [Reports an `id` used more than once on a page.],

  [`aria`],
  [flag],
  [Reports an unknown ARIA role or attribute, and one pointing at an id that is not there.],
)

```kdl
lint {
  strict
  alt #false
}
```

An empty `alt` is not a finding: that is how you say an image is decorative and
the page reads the same without it. `aria-hidden="true"` and `role="presentation"`
say the same thing. Derived heading anchors are deduplicated for you, so `ids`
only reports two you wrote yourself. The `doc-` and `graphics-` role vocabularies
are accepted by prefix, since they are real ARIA extension modules.

#callout(kind: "note")[
  Expect `headings` to have something to say the first time you turn it on.
  typst-html reserves `<h1>` for the document title, so a `=` in your content is
  an `<h2>` and a `==` is an `<h3>`. A layout that emits its own `<h1>` over
  pages opening at `==` goes `h1` straight to `h3`. Open your sections at `=`, or
  turn the rule off.
]

`links { strict }` is the same idea for broken internal links, and it defaults to
failing. That asymmetry is deliberate: a `.typ` link naming no page is a
certainty, while a missing `alt` is a judgement about content baudelaire did not
write.

== Outbound links

`links { external }` verifies every `http(s)` link the pages carry. It reaches
the network, so only `baudelaire check` runs it: a build produces the same bytes
offline, on a plane, and when somebody else's host is having a bad afternoon.

```kdl
links {
  external {
    fresh "7d"
    timeout "10s"
    concurrency 4
    ignore "*.internal/**" "staging.example.com/**"
    accept 401 429
  }
}
```

The block's presence turns the check on, so `links { external }` alone is the
whole of it; `external #false` turns it back off.

#table(
  columns: 3,
  align: (left, left, left),
  table.header([Key], [Default], [Does]),
  [`fresh`], [`7d`], [How long a link that answered is trusted before it is asked again.],
  [`timeout`], [`10s`], [How long one request may take before the link counts as unreachable.],
  [`concurrency`], [the build's threads], [How many links are fetched at once.],
  [`ignore`], [none], [Globs, matched against each URL without its scheme, never requested.],
  [`accept`], [none], [Status codes that count as alive, beyond 2xx and 3xx.],
)

A duration is a number and a unit (`250ms`, `30s`, `5m`, `2h`, `7d`); a bare
number is seconds.

Only successes are remembered, in `.baudelaire/links/seen.json`, so shortening
`fresh` re-asks and never keeps reporting a link you have already fixed. Lower
`concurrency` for a host that answers 429 to a site linking it fifty times; the
limit applies to these requests alone and leaves the rest of the build parallel.

`ignore` drops a URL before it is ever requested. The glob grammar is the one
`prune { keep }` uses, matched against the URL with its scheme removed, so
`*.internal/**` covers `http` and `https` alike and `/` is a segment boundary.

`accept` widens what counts as working. A page behind a login answers 401 and is
still there; a rate limiter answers 429 and says nothing about the link.

#callout(kind: "note")[
  A dead link fails the check. A link that could not be reached at all (DNS, TLS,
  a timeout) is only a warning: the most likely cause is the network in between,
  not the site.
]

== Budgets

A ceiling on what one page ships, written in bytes or in the units the build
summary prints (`50kB`, `1.5MB`, `0`).

```kdl
lint {
  budget {
    html "50kB"
    js 0
    css "20kB"
    images "300kB"
    total "400kB"
  }
}
```

#table(
  columns: 2,
  align: (left, left),
  table.header([Key], [Does]),
  [`html`], [Weighs the page's own markup, exactly as written to the output.],
  [`js`], [Weighs every script it loads, plus its inline `<script>` bodies.],
  [`css`], [Weighs every stylesheet it loads, plus its inline `<style>` bodies.],
  [`images`], [Weighs every image it references, responsive alternatives excluded.],
  [`total`], [Weighs its whole transfer weight: markup plus everything fetched alongside it.],
)

Every key is optional and each is checked per page. Responsive `srcset`
candidates don't count against `images`: they're alternatives to `src`, and a
visitor is served one of them. An inline script counts once in `total`, as part
of `html`, even though `js` counts it too. `js` answers how much JavaScript runs;
`total` answers how many bytes cross the wire.

Going over fails the build, unconditionally. Unlike a lint finding, a budget is a
limit you wrote down, and a limit that only warns is a number in a config file.

```text
× 1 page over budget
├─▶ posts/heavy.typ: `images` is 412.6 KiB, over the 300 KiB budget
╰─▶ help: ship less, or raise the limit under `lint { budget { } }`
```

A file this build did not write weighs nothing: a script pulled from a CDN, or
anything under `static/`, is somebody else's byte and counting it would be a
guess. An asset inlined by `html { embed }` is already inside the markup, so it
is billed to `html`.

#callout(kind: "warn")[
  `baudelaire check` compiles every page but processes no assets, so it can see
  *what* a page loads and not how large any of it is. It runs the rules and
  leaves the budgets to `baudelaire build`, which has the bytes.
]

== Elsewhere

Two more checks live under `links { }` rather than here, because neither is about
a page's own markup: broken internal links (`links { strict }`, see
#link("../write/pages.typ")[pages]) and the pages nothing links to
(`links { orphans }`, see #link("../write/backlinks.typ")[backlinks]).

== Under the cache

Both halves survive an #link("incremental.typ")[incremental build]. A page
records its findings and its weights alongside its markup, and a cache hit
replays them, so the second build of a site reports exactly what the first did.
Byte weights are read fresh from what this build emitted, so a fatter stylesheet
fails the pages that load it without any of them having changed.

Turning a rule on or changing a budget is a config change, so it rebuilds the
site once.
