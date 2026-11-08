#let frontmatter = (
  title: "Configuration",
  order: 2,
  summary: "Every key, in the order you meet them.",
  tags: ("reference",),
)

```toml
name = "wren"
output = "dist"

[build]
parallel = true
retries = 2
```

= `name`

What the thing is called. It is used in the page title and nowhere else.

= `output`

Where the build writes. The path is relative to the working directory, not to
this file.

= `build.parallel`

Whether to use every core. Turn it off while profiling or reading a stack
trace.

= `build.retries`

How many times a step may fail before the build does. Zero fails on the first
error.
