#let frontmatter = (
  title: "Hooks",
  order: 1,
  summary: "A nested section, so the sidebar has something to nest.",
)

This page sits in `content/guide/advanced/`, one level deeper than its
neighbours, so the theme shows a subdirectory as a collapsible group inside the
group above it.

= Before and after

```toml
[hooks]
before = "wren check"
after = "wren publish"
```

A collapsed group stays collapsed on the next page, unless the current page is
inside it, so the reader's own position is never hidden.
