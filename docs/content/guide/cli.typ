#let frontmatter = (
  title: "CLI reference",
  order: 5,
)

Every command reads `config.kdl` from the current directory (or the path given
to `--config`) and accepts the #link(<global-flags>)[global flags] below.

== Commands

At a glance:

#table(
  columns: 2,
  align: (left, left),
  table.header([Command], [What it does]),
  [`baudelaire build`], [Compile the site into `dist`. The default command.],
  [`baudelaire serve`], [Build, serve, watch, and live-reload.],
  [`baudelaire check`], [Compile and report broken links without writing.],
  [`baudelaire new <path>`], [Scaffold a content file from config and convention.],
  [`baudelaire publish`], [Push the built site to every destination.],
  [`baudelaire init [dir]`], [Scaffold a whole project.],
  [`baudelaire clean`], [Remove build output and local state.],
)

=== build

Compile the site into `dist`, incrementally by default. This is the default
command, so bare `baudelaire` builds too. Accepts the
#link(<build-flags>)[build flags].

=== serve

Build, serve `dist` over HTTP, watch the sources for changes, and live-reload
the browser. Also accepts the #link(<build-flags>)[build flags].

/ #raw("--port <n>"): Port to listen on.
/ #raw("--bind <addr>"): Address to bind.
/ #raw("--open"): Open the site in your browser once it's up.
/ #raw("--no-watch"): Serve once, without watching or live-reload.

=== check

Compile every page and report broken internal links without writing output — a
fast CI gate. Accepts the #link(<build-flags>)[build flags].

=== new \<path>

Scaffold a content file, inferring its structure from the config and existing
content. A bare name lands under the content directory, so
`baudelaire new posts/hello` writes `content/posts/hello.typ`.

What it infers:

- The title from the filename (`my-first-post` → "My First Post").
- The ordering field from the collection: a `date` for a `sort="date"`
  collection, the next `order` for a `sort="order"` one.
- The template and the permalink the page will occupy — warning if that URL is
  already taken.

/ #raw("--title <text>"): Override the inferred title.
/ #raw("--date YYYY-MM-DD"): Set the date explicitly.
/ #raw("--draft <bool>"): Mark the page a draft.
/ #raw("-b, --bundle"): Create `<name>/index.typ` for colocated assets.
/ #raw("-e, --open"): Open the new file in `$EDITOR`.

=== publish

Publish the built site to every configured destination.

/ #raw("--password <pw>"): `-` reads it from stdin; prefer that or the
  environment variable over a literal flag.
/ #raw("-y, --yes"): Skip the confirmation prompt.
/ #raw("--dry-run"): Report what a real publish would send and remove, without
  writing. Needs no password — it diffs against the live repository over public
  reads.

=== init \[dir\]

Scaffold a whole project — config, a layout, a starter page and post, and a
stylesheet — into `dir`, or the current directory. It prompts for the site
name, author (defaulted from your git config), and base URL, then offers to set
up version control with a `.gitignore`.

/ #raw("--vcs <git|jujutsu>"): Choose a VCS instead of being prompted.
/ #raw("-y"): Accept every default non-interactively.

=== clean

Remove build output and local build state. With no flag it sweeps everything:
the output plus the `.baudelaire` scratch root (cache and publish state). The
flags narrow it, so `clean --cache` forces a rebuild without discarding publish
state.

/ #raw("--dist"): Remove only the output directory.
/ #raw("--cache"): Remove only the build cache.
/ #raw("--publish"): Remove only the publish state.

== Global flags <global-flags>

Accepted by every command.

/ #raw("--config <path>"): Config file to read (default `config.kdl`).
/ #raw("--root <dir>"): Change into `dir` first, so every relative path
  resolves under it.
/ #raw("--profile <name>"): Apply a named profile from the `profiles` block.
/ #raw("-v"), #raw("-q"): More (`-v`, repeat for deeper logs) or less output.

== Build flags <build-flags>

Config overrides accepted by the commands that build — `build`, `serve`, and
`check`.

/ #raw("--out <dir>"): Override the output directory.
/ #raw("--base-url <url>"): Override the site URL, useful for preview deploys.
/ #raw("--drafts"), #raw("--future"): Include draft or future-dated pages.
/ #raw("--no-cache"): Ignore the cache and rebuild everything.
/ #raw("--strict-links <bool>"): Treat broken internal links as errors
  (default) or warnings.
