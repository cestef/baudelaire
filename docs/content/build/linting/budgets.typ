#let frontmatter = (
  title: "Budgets",
  order: 1,
)
#import "/templates/theme.typ": callout

A ceiling on what one page ships, written in bytes or in the units the build
summary prints (`50kB`, `1.5MB`, `0`).

```kdl
check {
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
check {
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
╰─▶ help: ship less, or raise the limit under `check { budget { } }`
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
