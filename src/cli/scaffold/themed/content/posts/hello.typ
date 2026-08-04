#let frontmatter = (
  title: "Hello World",
  slug: "hello-world",
  date: datetime(year: 2024, month: 1, day: 1),
  // A single-element Typst array needs the trailing comma. Without it this is
  // the string "intro", not a list of one tag.
  tags: ("intro", "typst"),
  // Frontmatter keys Baudelaire does not claim are yours to use. This one shows
  // up in listings as `entry.extra.summary`.
  summary: "What you get when a post is a Typst document rather than markdown.",
)

This is a post. It is also a Typst document, so the whole language is here:
math like $e^(i pi) + 1 = 0$, and functions you define where you need them.

#let half-life(name, years) = [#name decays with a half-life of #years years.]
#half-life("Carbon-14", 5730)

The theme decides how this looks. Nothing in the frontmatter above names a
layout, because the collection this page lands in binds one, and the theme is
what declares that collection.
