#let frontmatter = (
  order: 26,
  title: "Linting",
  tags: ("feature", "linting"),
)
#import "/templates/theme.typ": callout

Baudelaire post-processes each page as a typed DOM, never as a string, so it can
be asked questions about the page it is about to write: does the outline skip a
heading level, does that image say anything to a screen reader, is that `id`
already taken, does that `aria-labelledby` point at anything. And because the
DOM still carries the Typst span every element came from, a finding is reported
against the line you wrote, not against a byte offset in generated markup.

Nothing is linted until you ask. Add a `lint` block:

```kdl
lint { }
```

Its presence turns every rule on. Findings are warnings; the build still
succeeds.

== Strictness

```kdl
lint { strict }
```

Now a finding fails the build, exactly as `links { strict }` does for a broken
internal link. That asymmetry is the default on purpose: a `.typ` link naming no
page is a certainty, while a missing `alt` is a judgement about content
baudelaire did not write.

== The rules

Each is a flag, on while the block is present, off by name:

/ `headings`: a heading that skips a level, `h2` straight to `h4`. Assistive
  technology navigates a page by level, and a section introduced under a level
  that was never opened has no place in that outline. Coming back *up* any
  number of levels is fine: that is how a document ends a chapter.
/ `alt`: an `<img>` with no `alt` attribute at all. An empty `alt` is not a
  finding: it is the way to say the image is decorative and the page reads the
  same without it. So is `aria-hidden="true"` or `role="presentation"`.
/ `ids`: an `id` used more than once. A repeated one silently breaks every deep
  link and every `aria-labelledby` into it, since both reach only the first.
  Derived heading anchors are deduplicated for you; two ids you wrote are yours.
/ `aria`: a `role` that is not a WAI-ARIA role, an `aria-*` attribute ARIA does
  not define, and an `aria-labelledby` (or `-describedby`, `-controls`,
  `-owns`, ...) naming an id that is not on the page. All three read as working
  markup and none of them reaches the accessibility tree.

```kdl
lint {
  strict
  alt #false
}
```

#callout(kind: "note")[
  The `doc-` and `graphics-` role vocabularies are accepted by prefix: they are
  ARIA extension modules, and treating one as a typo would fail a correct page.
]

Expect `headings` to have something to say the first time you turn it on.
typst-html reserves `<h1>` for the document title, so a `=` in your content is
an `<h2>`, a `==` an `<h3>`, and so on. A site whose layout emits its own `<h1>`
and whose pages open at `==` therefore goes `h1` straight to `h3`, which is a
real skip in the served page whatever it looked like in the source. Open your
sections at `=`, or turn the rule off.

== Budgets

A budget is a ceiling on what one page ships, written in bytes or in the units
the build summary prints:

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

Every key is optional and each is checked per page:

/ `html`: the page's own markup, exactly as written to `dist`.
/ `js`: every script it loads, plus the bytes of its inline `<script>` bodies.
/ `css`: every stylesheet it loads, plus its inline `<style>` bodies.
/ `images`: every image it references. Responsive `srcset` candidates are *not*
  counted: they are alternatives to `src`, and a visitor is served one of them.
/ `total`: the page's whole transfer weight: its markup plus everything fetched
  alongside it. An inline script counts once here, as part of `html`, even
  though `js` also counts it: `js` answers how much JavaScript runs, `total`
  answers how many bytes cross the wire.

A page over budget fails the build, and does so unconditionally: unlike a lint
finding, a budget is a limit you wrote down, and a limit that only warns is a
number in a config file. The report names the page, the budget, what it weighed
and what it was allowed:

```
× 1 page over budget
├─▶ posts/heavy.typ: `images` is 412.6 KB, over the 300 KB budget
╰─▶ help: ship less, or raise the limit under `lint { budget { } }`
```

A file this build did not write weighs nothing: a script pulled from a CDN, or
anything under `static/`, is somebody else's byte and counting it would be a
guess. An inlined asset (`html { embed }`) is already inside the markup, so it
is billed to `html`.

#callout(kind: "warning")[
  `baudelaire check` compiles every page but processes no assets, so it can see
  *what* a page loads and not how large any of it is. It runs the rules and
  leaves the budgets to `baudelaire build`, which has the bytes.
]

== In a cache

Both halves survive an incremental build. A page records what it found and what
it ships alongside its markup, and a cache hit replays them, so the second build
of a site reports exactly what the first did. Turning a rule on or changing a
budget is a config change, and so rebuilds the site once.
