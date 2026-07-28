#let frontmatter = (
  title: "Hello, world",
  date: datetime(year: 2026, month: 7, day: 28),
  tags: ("meta", "typst"),
  summary: "A page with nothing to say, saying it in every way Typst can. Headings, lists, tables, math, highlighted code, loaded data, computed markup, and a callout, all from one compiler.",
)
#import "/templates/theme.typ": callout

This post has no argument to make. It exists so every part of the rendering
pipeline has something to render.

== Text

Regular text, #emph[emphasis], #strong[strong], `inline raw`, #underline[underline],
#strike[strike], #highlight[highlight], H#sub[2]O and E = mc#super[2]. A
#link("https://typst.app")[link out], and a #link("../guide/quickstart.typ")[link to
another page] that the build resolves and checks.

#quote(block: true, attribution: [nobody in particular])[
  A block quote, attributed.
]

== Lists

- A bullet
- Another bullet
  - Nested one level

+ First
+ Second
+ Third

/ Term: A term list, which Markdown does not have.
/ Another: And its definition.

== Code

Fenced blocks are highlighted by the compiler, not by a client-side script:

```rust
pub fn slug(name: &str) -> String {
    name.chars()
        .map(|c| if c.is_alphanumeric() { c.to_ascii_lowercase() } else { '-' })
        .collect()
}
```

```kdl
content {
  collections {
    blog sort="date" reverse=#true paginate=5 list="list.typ"
  }
}
```

```typ
#let frontmatter = (title: "Hello, world")
```

== Math

Inline, $a^2 + b^2 = c^2$, and displayed:

$ sum_(k = 1)^n k = (n (n + 1)) / 2 $

$ integral_0^oo e^(-x^2) dif x = sqrt(pi) / 2 $

== Tables

#table(
  columns: 3,
  table.header([Format], [File], [Standard]),
  [RSS], [`rss.xml`], [RSS 2.0],
  [Atom], [`atom.xml`], [RFC 4287],
  [JSON], [`feed.json`], [JSON Feed 1.1],
)

== A page is a program

The three things above are markup. This is not: the table below is a `for` loop
over a list, and the list came from a string the compiler parsed at build time.

#let rows = csv(
  bytes(
    "crate,job
typst,compile
rolldown,bundle
lightningcss,minify
blake3,fingerprint",
  ),
)

#table(
  columns: 2,
  table.header(..rows.first().map(c => strong(upper(c.slice(0, count: 1)) + c.slice(1)))),
  ..rows.slice(1).flatten().map(cell => raw(cell)),
)

A function is a function, so markup can be generated:

#let badge(label) = box(inset: (x: 4pt), radius: 3pt, raw(label))

#for n in range(1, 6) [#badge("v0." + str(n)) ]

== Callout

#callout(kind: "tip", label: "Nothing here")[
  Every block on this page is a feature test. If one of them stops rendering, the
  build noticed before you did.
]
