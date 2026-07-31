#let frontmatter = (
  title: "Hooks",
  order: 1,
  summary: "A nested section, so the sidebar has something to nest.",
)

This page sits in `content/guide/advanced/`, one level deeper than its
neighbours, which is the only reason it exists: a documentation theme has to
show what it does with a subdirectory, and the answer is a collapsible group
inside the group above it.

= Before and after

```toml
[hooks]
before = "wren check"
after = "wren publish"
```

A reader who collapses this group keeps it collapsed on the next page, except
when the page they are on lives inside it. Hiding someone's own position is the
one unhelpful thing a nav can do.
