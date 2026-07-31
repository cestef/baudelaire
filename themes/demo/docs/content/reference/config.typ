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

What the thing is called. Used in the page title and in nothing else, which is
a small mercy.

= `output`

Where the build writes. Relative to the working directory, not to this file,
because that is the mistake everyone makes once.

= `build.parallel`

Whether to use every core. Off is useful exactly twice: while profiling, and
while reading a stack trace.

= `build.retries`

How many times a step may fail before the build does. Zero is honest; two is
what people set after a flaky afternoon.
