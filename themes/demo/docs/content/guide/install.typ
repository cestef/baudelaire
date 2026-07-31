#let frontmatter = (
  title: "Install",
  order: 1,
  summary: "Three ways to get a binary that does not exist.",
  tags: ("setup",),
)

Wren ships as one static binary, which is easy to promise for a tool nobody has
written.

= From a release

```sh
curl -fsSL https://example.invalid/install.sh | sh
```

= From source

```sh
cargo install wren --locked
```

The `--locked` matters: it builds against the lockfile in the published crate
rather than resolving fresh dependencies, so two people on the same version get
the same binary.

= Verifying

```sh
wren --version
```

If that prints nothing, the binary is not on your `PATH`, which is the usual
answer and rarely the interesting one.
