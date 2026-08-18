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

Each is on while the block is present, and off by name.

#table(
  columns: 2,
  align: (left, left),
  table.header([Key], [Does]),
  [`headings`], [Reports a heading that skips a level, `h2` straight to `h4`.],
  [`alt`], [Reports an `<img>` with no `alt` attribute at all.],
  [`ids`], [Reports an `id` used more than once on a page.],
  [`aria`], [Reports an unknown ARIA role or attribute, and one pointing at an id that is not there.],
  [`snippets`], [Checks each code fence as the language it claims. One line per language; see #link(<snippets>)[below].],
)

=== How loud each one is

`strict` is the default the rules follow, not an override. A rule that names its
own severity keeps it, so a site can hold everything to `error` and still let one
report:

```kdl
lint {
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
  Expect `headings` to have something to say the first time you turn it on.
  typst-html reserves `<h1>` for the document title, so a `=` in your content is
  an `<h2>` and a `==` is an `<h3>`. A layout that emits its own `<h1>` over
  pages opening at `==` goes `h1` straight to `h3`. Open your sections at `=`, or
  say which level yours open at:

  ```kdl
  lint {
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
  [`strict`], [Whether going over fails the build. On by default.],
)

Every key is optional and each is checked per page. Responsive `srcset`
candidates don't count against `images`: they're alternatives to `src`, and a
visitor is served one of them. An inline script counts once in `total`, as part
of `html`, even though `js` counts it too. `js` answers how much JavaScript runs;
`total` answers how many bytes cross the wire.

Going over fails the build. Unlike a lint finding, a budget is a limit you wrote
down, and a limit that only warns is a number in a config file.

Adopting one on a site that already has pages is the exception, and it says so:

```kdl
lint {
  budget {
    strict #false
    html "50kB"
  }
}
```

That reports the same pages without failing, so a number can be a target before
it is a limit.

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

== Code fences <snippets>

A fence claims a language. `snippets` is where a site says what that claim is
worth: nothing until a language is named, and from then on every fence of it is
read by something that knows the language.

```kdl
lint {
  snippets {
    kdl  run="$BAUDELAIRE config check --isolated --compact {file}"
    json
    typ "warn"
    sh   run="shellcheck -s sh {file}"
  }
}
```

A line with no `run` is checked by the parser this binary already carries:

#table(
  columns: 2,
  align: (left, left),
  table.header([Language], [Read by]),
  [`kdl`], [The KDL parser, for syntax alone.],
  [`json`], [`serde_json`.],
  [`toml`], [`toml_edit`.],
  [`yaml`, `yml`], [`saphyr`.],
  [`typ`, `typst`], [Typst's own parser, which parses without evaluating.],
)

Naming any other language without a `run` is a config error rather than a line
that checks nothing.

#table(
  columns: 3,
  align: (left, left, left),
  table.header([Key], [Type], [Does]),
  [`run`], [str], [The command that checks one snippet. `{file}` is the snippet on disk, as one shell word, and `{lang}` the language it claimed. Nonzero exit is a finding; what the command wrote is the message.],
  [`hidden`], [str], [A line prefix that is checked and never shown, for the context a fragment needs. See #link(<hidden>)[hidden lines].],
)

A `run` command is run through the system shell in the project root, like a
#link("hooks.typ")[hook], and `$BAUDELAIRE` is the binary running the build, so a
site checks its own config examples with the very build that renders them rather
than with whatever is on `PATH`. A command that reports `file:line:column:` has
its message placed on that line of the fence; one that reports anything else is
reported against the fence itself.

=== Hidden lines <hidden>

A documentation fence usually shows a fragment: the block that matters, not the
three blocks around it. `hidden` is the prefix that carries the rest:

````typ
```kdl
//! content { collections { posts {
schema { title "str" }
//! } } }
```
````

The reader sees the `schema` block. The checker reads all five lines. The prefix
is yours to pick, and a comment in the language keeps the raw file readable to
anyone who never renders it.

One line is a directive rather than context: `//! @ignore` (the prefix, then
`@ignore`) says this fence is not meant to check at all, which is what a snippet
that is wrong on purpose needs.

== Keeping a rule off one place

A finding can be right about the markup and wrong about the page. `nolint` marks
a region the lint does not look at:

```typ
#import "@baudelaire/html:0.1.0": nolint

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

Two more checks are documented elsewhere, because neither is about a page's own
markup: broken internal links (`links { strict }`, see
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
