#let frontmatter = (
  title: "First steps",
  slug: "first-steps",
  order: 2,
)

Adding a chapter is one file. Copy this one to `content/chapters/03-whatever.typ`,
give it a `title`, a `slug` and the next `order`, and write. There is nothing to
register: the collection is a directory, so the contents list, the pager and the
one-file export all pick the chapter up on the next build.

```typ
#let frontmatter = (
  title: "Whatever comes next",
  slug: "whatever",
  order: 3,
)
```

Two chapters may not share an `order`; give one of them a number in between
rather than renumbering everything after it. A chapter you are not ready to show
goes in `03-whatever.draft.typ` instead: it builds under `--profile dev`, and is
left out of every other build.

= Why the source is Typst

A book is where the difference between a markup language and a typesetting
language stops being academic. Markdown gives you headings, emphasis and lists.
Past that you are pasting rendered images of equations, numbering your own
figures, and keeping a table of contents in sync by hand.

Everything below is written in the chapter source, compiled once, and rendered as
part of the page.

== Mathematics

Inline, so it sits in a sentence: the golden ratio is $phi = (1 + sqrt(5)) / 2$,
and it is the positive root of $x^2 - x - 1 = 0$. Displayed, when it is the point
of the paragraph rather than an aside:

$ sum_(k = 0)^n binom(n, k) x^k y^(n - k) = (x + y)^n $

$ integral_(-oo)^(oo) e^(-x^2) dif x = sqrt(pi) $

No plugin, no client-side renderer, no screenshots of equations that go blurry on
a phone. The compiler that lays out the text lays out the mathematics.

== Figures, numbered and captioned

#figure(
  table(
    columns: 3,
    table.header([Piece], [Where it comes from], [Kept in step by]),
    [Contents], [`page.sections`], [the collection's `sort`],
    [Pager], [`page.nav`], [the same `sort`],
    [Anchors], [`html { anchors }`], [each heading's own text],
  ),
  caption: [Nothing in this book's navigation is written down twice.],
)

The caption and the number are the figure's own, so inserting a figure earlier in
the chapter renumbers the rest.

== Code, as part of the argument

#figure(
  ```rust
  /// The chapter after this one, or `None` at the end of the book.
  pub fn next(&self, order: u32) -> Option<&Chapter> {
    self.chapters.iter().find(|c| c.order > order)
  }
  ```,
  caption: [Highlighted at build time, by the compiler, from the fence's language tag.],
)

Highlighting happens during the build, so the page ships no syntax highlighter
and the code is still coloured inside `book.html` when it is opened from a USB
stick on a machine that has never heard of this site.

== A chapter is a program

Markup is only the surface. The chapter file is a Typst module, so anything that
would otherwise be a hand-maintained list can be computed:

#let editions = ((1, 2019), (2, 2022), (3, 2026))

#for (n, year) in editions [
  - Edition #n, published #year.
]

Load a CSV of results and render it as a table, generate an exercise set from a
function, define a `#theorem` that numbers itself. The page is built by a
compiler, not templated by string substitution, so anything the compiler can do
is available to a chapter.

= Making it yours

- `assets/style.css` holds every rule this book uses; there is no framework
  underneath to fight.
- `templates/theme.typ` is the frame: the header, the contents list, the pager.
- Set `content { collections { chapters permalink="/{slug}/" } }` in `config.kdl`
  to drop the `/chapters/` prefix from every chapter's URL.
- `navigation { standalone { file } }` names the export; `book.html` is only a
  default.

Then `baudelaire serve` while you write, and `baudelaire build` when you are
done.
