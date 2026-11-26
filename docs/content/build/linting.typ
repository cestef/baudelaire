#let frontmatter = (
  title: "Linting",
  order: 6,
)
#import "/templates/theme.typ": callout

Accessibility checks over the built pages, and a byte ceiling on what each one
ships.

```kdl
check {
  strict
  budget {
    html "50kB"
    css "20kB"
    js 0
  }
}
```

Nothing is linted until the block is there. Its presence turns every markup rule
on, and `check #false` turns them all off again, which is how a profile says it.
Findings are warnings: the build still succeeds. `strict` makes a finding fail it
instead. The link rules below answer for themselves either way, so
`check #false { links "warn" }` is a site that checks its links and lints
nothing.

Every element still carries the Typst source span it came from, so a finding is reported against the line you wrote, not a byte offset in generated markup.

== The rules

Each is on while the block is present, and off by name.

#table(
  columns: 2,
  align: (left, left),
  table.header([Key], [Does]),
  [`headings`], [Reports a heading that skips a level, `h2` straight to `h4`.],
  [`alt`], [Reports an `<img>` with no `alt` attribute at all.],
  [`ids`], [Reports an `id` used more than once on a page.],
  [`aria`], [Reports an unknown ARIA role or attribute, and one pointing at an id that is not there.],
  [`snippets`], [Checks each code fence as the language it claims. One line per language; see #link("linting/fences.typ")[Code fences].],
)

=== How loud each one is

`strict` is the default severity. A rule that names its own severity keeps it, so a site can hold everything to `error` and still let one
report:

```kdl
check {
  strict
  headings "warn"
  alt #false
}
```

#table(
  columns: 2,
  align: (left, left),
  table.header([Value], [Means]),
  [`#true`], [On, at whatever `strict` says. The spelling a rule has always taken.],
  [`#false`], [Off. The same as `"off"`.],
  [`"off"`], [The rule does not run.],
  [`"warn"`], [A finding is reported and the build succeeds.],
  [`"error"`], [A finding fails the build, whatever `strict` says.],
)

An empty `alt` is not a finding: that is how you say an image is decorative and
the page reads the same without it. `aria-hidden="true"` and `role="presentation"`
say the same thing. Derived heading anchors are deduplicated for you, so `ids`
only reports two you wrote yourself. The `doc-` and `graphics-` role vocabularies
are accepted by prefix, since they are real ARIA extension modules.

#callout(kind: "note")[
  `headings` often has findings the first time you turn it on. Typst's HTML export reserves `<h1>` for the document title, so a `=` in your content is
  an `<h2>` and a `==` is an `<h3>`. A layout that emits its own `<h1>` over
  pages opening at `==` goes `h1` straight to `h3`. Open your sections at `=`, or
  say which level yours open at:

  ```kdl
  check {
    headings {
      start 3
    }
  }
  ```

  Only the heading right under the layout's own may land there; a skip further
  down the page is still reported, so an `h3` page jumping to `h5` still fails.
  `headings "warn"` is the shorthand for `headings { level "warn" }`, and both
  spellings take a block.
]

== Internal links

`links` is the same kind of rule for a `.typ` link that names no page, and it
takes the same severities. It defaults to `"error"` rather than following `strict`: a link naming no page is certainly wrong, while a missing `alt` is a judgement about content baudelaire did not write.

```kdl
check {
  links "warn"
}
```

It is read whether or not the markup rules are on, so a site that lints nothing
still gets its links checked.

== Outbound links

`external` verifies every `http(s)` link the pages carry. It uses the network, so only `baudelaire check` runs it: a build produces the same bytes offline and when somebody else's host is down.

```kdl
check {
  external {
    fresh "7d"
    timeout "10s"
    concurrency 4
    ignore "*.internal/**" "staging.example.com/**"
    accept 401 429
  }
}
```

The block's presence turns the check on, so `check { external }` alone enables it; `external #false` turns it back off.

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
`fresh` re-asks and never keeps reporting a link you have already fixed.
Narrowing `accept` re-asks too: what the host answered is remembered beside when
it answered, and both are judged again under the settings of the run that reads
them. Lower `concurrency` for a host that answers 429 to a site linking it fifty
times; the limit applies to these requests alone and leaves the rest of the build
parallel.

`ignore` drops a URL before it is ever requested. The glob grammar is the one
`prune { keep }` uses, matched against the URL with its scheme removed, so
`*.internal/**` covers `http` and `https` alike and `/` is a segment boundary.

`accept` widens what counts as working. A page behind a login answers 401 and is
still there; a rate limiter answers 429 and says nothing about the link.

#callout(kind: "note")[
  A dead link fails the check. A link that could not be reached at all (DNS, TLS,
  a timeout) is only a warning, since the likely cause is the network in between.
]

== Budgets

A ceiling on what one page ships, in bytes: #link("linting/budgets.typ")[Budgets].
== Code fences <snippets>

`snippets` says what each fenced language is checked by: #link("linting/fences.typ")[Code fences].
== Keeping a rule off one place

A finding can be correct about the markup and still wrong for the page. `nolint` marks
a region the lint does not look at:

```typ
#import "/.baudelaire/site/html.typ": nolint

#nolint[
  ==== A section that starts deep on purpose
]

#nolint("headings", "alt")[..]   // only those two
```

Everything inside is invisible to the rules it names, or to every rule when it
names none, native Typst elements included. The marker is read and removed
before the page is written, so the output is the one you would have had without
it.

#callout(kind: "note")[
  The marker is an attribute, `data-lint`, so markup that builds its own
  elements can write it directly: `h("div", data-lint: "headings")[..]`. `off`
  is every rule.
]

== Elsewhere

Two more checks are documented elsewhere, because neither concerns a page's own markup: broken internal links (`check { links }`, see
#link("../write/pages.typ")[pages]) and the pages nothing links to
(`check { orphans }`, see #link("../write/backlinks.typ")[backlinks]).

== Under the cache

The lint findings and the budgets both survive an #link("incremental.typ")[incremental build]. A page
records its findings and its weights alongside its markup, and a cache hit
replays them, so the second build of a site reports exactly what the first did.
Byte weights are read fresh from what this build emitted, so a fatter stylesheet
fails the pages that load it without any of them having changed.

Turning a rule on or changing a budget is a config change, so it rebuilds the
site once.
