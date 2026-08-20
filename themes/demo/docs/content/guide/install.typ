#let frontmatter = (
  title: "Install",
  order: 1,
  summary: "Three ways to get a binary that does not exist.",
  tags: ("setup",),
  updated: datetime(year: 2026, month: 8, day: 12),
)
// The preview builds the theme from a directory in this repository, so its
// package entrypoint is a project path. A site that installed the published
// theme writes `#import "@preview/phares:0.1.0": ..` instead.
#import "/themes/phares/lib.typ": badge, callout, pane, steps, tabs

Wren ships as one static binary, which is easy to promise for a tool nobody has
written.

= Pick a channel

#tabs(
  pane("release")[
    ```sh
    curl -fsSL https://example.invalid/install.sh | sh
    ```
  ],
  pane("cargo")[
    ```sh
    cargo install wren --locked
    ```

    The `--locked` matters: it builds against the lockfile in the published
    crate rather than resolving fresh dependencies, so two people on the same
    version get the same binary.
  ],
  pane("brew")[
    ```sh
    brew install example/tap/wren
    ```
  ],
)

= Then #badge("2 min")

#steps[
  + Put the binary on your `PATH`.

  + Check that the shell can see it:

    ```sh
    wren --version
    ```

  + Start a site:

    ```sh
    wren init my-site
    ```
]

#callout(kind: "warning", title: "If that prints nothing")[
  The binary is not on your `PATH`, which is the usual answer and rarely the
  interesting one.
]
