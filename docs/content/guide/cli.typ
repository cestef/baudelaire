#let frontmatter = (
  title: "CLI reference",
  order: 5,
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
/ #raw("baudelaire new <path>"): Scaffold a content file, inferring its
  structure from the config and existing content: the title from the filename
  (`my-first-post` → "My First Post"), the ordering field from the collection (a
  `date` for a `sort="date"` collection, the next `order` for a `sort="order"`
  one), the template, and the permalink it will occupy — warning if that URL is
  already taken. A bare name lands under the content directory, so
  `baudelaire new posts/hello` writes `content/posts/hello.typ`. Flags:
  `--title`, `--date YYYY-MM-DD`, `--draft <bool>`, `-b/--bundle` (create
  `<name>/index.typ` for colocated assets), and `-e/--open` (open in `$EDITOR`).
/ #raw("baudelaire publish"): Publish the built site to every configured
  destination. Flags: `--password` (`-` reads it from stdin; prefer that or the
  environment variable over a literal flag), `-y/--yes` (skip the confirmation),
  and `--dry-run` (report what would change without writing).
/ #raw("baudelaire init [dir]"): Scaffold a whole project (config, a layout, a
  starter page and post, and a stylesheet) into `dir`, or the current directory.
  It prompts for the site name, author (defaulted from your git config), and
  base URL, then offers to set up version control with a `.gitignore`. Pass
  `--vcs git` or `--vcs jujutsu` to choose a VCS, or `-y` to accept every
  default non-interactively.
/ #raw("baudelaire clean"): Remove build output and local build state. With no
  flag it sweeps everything (the output plus the `.baudelaire` scratch root:
  cache and publish state). Narrow it with `--dist`, `--cache`, or `--publish`,
  so `clean --cache` forces a rebuild without discarding publish state.

== Global flags

Accepted by every command.

/ #raw("--config <path>"): Config file to read (default `config.kdl`).
/ #raw("--root <dir>"): Change into `dir` first, so every relative path
  resolves under it.
/ #raw("--profile <name>"): Apply a named profile from the `profiles` block.
/ #raw("-v"), #raw("-q"): More (`-v`, repeat for deeper logs) or less output.

== Build flags

Config overrides accepted by the commands that build — `build`, `serve`, and
`check`.

/ #raw("--out <dir>"): Override the output directory.
/ #raw("--base-url <url>"): Override the site URL, useful for preview deploys.
/ #raw("--drafts"), #raw("--future"): Include draft or future-dated pages.
/ #raw("--no-cache"): Ignore the cache and rebuild everything.
/ #raw("--strict-links <bool>"): Treat broken internal links as errors
  (default) or warnings.
