#let frontmatter = (
  title: "CLI reference",
  order: 4,
  tags: ("guide", "reference"),
)

Every command reads `config.kdl` from the current directory (or the path given
to `--config`) and accepts the global flags below.

== Commands

/ #raw("baudelaire build"): Compile the site into `dist`. Incremental by
  default. This is the default command, so bare `baudelaire` builds too.
/ #raw("baudelaire serve"): Build, serve `dist` over HTTP, watch for changes,
  and live-reload the browser. Flags: `--port`, `--bind`, `--open`,
  `--no-watch`.
/ #raw("baudelaire check"): Compile every page and report broken internal links
  without writing output. A fast CI gate.
/ #raw("baudelaire new <path>"): Scaffold a content file with starter
  frontmatter, e.g. `baudelaire new content/posts/hello.typ`.
/ #raw("baudelaire init [dir]"): Scaffold a whole project (config, a layout, a
  starter page and post, and a stylesheet) into `dir`, or the current directory.
  It prompts for the site name, author (defaulted from your git config), and
  base URL, then offers to set up version control with a `.gitignore`. Pass
  `--vcs git` or `--vcs jujutsu` to choose a VCS, or `-y` to accept every
  default non-interactively.
/ #raw("baudelaire clean"): Remove the output and cache directories.

== Global flags

/ #raw("--config <path>"): Config file to read (default `config.kdl`).
/ #raw("--root <dir>"): Change into `dir` first, so every relative path
  resolves under it.
/ #raw("--profile <name>"): Apply a named profile from the `profiles` block.
/ #raw("--out <dir>"): Override the output directory.
/ #raw("--base-url <url>"): Override the site URL, useful for preview deploys.
/ #raw("--drafts"), #raw("--future"): Include draft or future-dated pages.
/ #raw("--no-cache"): Ignore the cache and rebuild everything.
/ #raw("--strict-links <bool>"): Treat broken internal links as errors
  (default) or warnings.
/ #raw("-v / -vv"), #raw("-q"): More or less output.
