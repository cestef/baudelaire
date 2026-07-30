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
  [`baudelaire deploy`], [Build, then upload the output to the configured S3 or SSH target.],
  [`baudelaire announce`], [Announce the site's metadata to atproto (standard.site).],
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
/ #raw("--open"): Open the site in your browser once it's up (the default;
  `--no-open` suppresses it).
/ #raw("--watch"): Rebuild on change and live-reload (the default; `--no-watch`
  serves once, statically).

=== check

Compile every page and report broken internal links without writing output: a
fast CI gate. "Internal links" means links written against a `.typ` source path
(the typst-native `#link("/content/post.typ")` cross-reference that resolves to
the target's permalink); a link to an already-resolved URL like `/posts/hello/`
is left untouched and never checked, so a permalink can change without breaking
`.typ` references to it. Accepts the #link(<build-flags>)[build flags].

A `#fragment` on such a link is checked too: `#link("/content/guide.typ#setup")`
has to find a heading with that `id` on the target page, so renaming a heading
reports every deep link into it instead of quietly breaking them. This runs
site-wide on every build, including over pages served from the cache, so
renaming a heading in one page is caught even though the page linking to it did
not change.

/ #raw("--external"): Also verify outbound `http(s)` links over the network
  (`--no-external` skips them even when `links { external #true }` is set).
  Every distinct URL is requested once (`HEAD`, retried as `GET` by servers that
  refuse the method) and a URL that answered is remembered for a week, so a
  repeat run only checks what it has not seen. A host that answers 4xx or 5xx
  fails the check; one that cannot be reached at all is a warning, since the
  likeliest cause is the network in between. Set `links { external #true }` to
  have CI do it without the flag.

Builds never reach the network, whatever `links { external }` says: a build has
to produce the same bytes offline and when someone else's host is having a bad
afternoon.

=== new \<path>

Scaffold a content file, inferring its structure from the config and existing
content. A bare name lands under the content directory, so
`baudelaire new posts/hello` writes `content/posts/hello.typ`.

What it infers:

- The title from the filename (`my-first-post` becomes "My First Post").
- The ordering field from the collection: a `date` for a `sort="date"`
  collection, the next `order` for a `sort="order"` one.
- The template and the permalink the page will occupy, warning if that URL is
  already taken.

/ #raw("--title <text>"): Override the inferred title.
/ #raw("--date YYYY-MM-DD"): Set the date explicitly.
/ #raw("--draft"): Mark the page a draft. On by default, so a new page stays out
  of a normal build until you finish it; pass `--no-draft` to publish it
  immediately, or build with `--drafts`.
/ #raw("-b, --bundle"): Create `<name>/index.typ` for colocated assets.
/ #raw("-e, --open"): Open the new file in `$EDITOR`.

=== deploy

Build the site, then upload the output to the destination in the `deploy` config
block: an #link("deploy/s3.typ")[S3-compatible bucket] or an
#link("deploy/ssh.typ")[SSH/SFTP host]. It uploads the built files; it does not
announce metadata (see #link("deploy/overview.typ")[Deploying] for the full
picture). Only changed files are sent, and stale remote files are pruned unless
`delete` is off. Errors if no `deploy` block is configured.

/ #raw("--secret <value>"): The destination secret (S3 secret key or SSH
  password/passphrase). `-` reads it from stdin; prefer that or the environment
  variable over a literal flag.
/ #raw("-y, --yes"): Skip the confirmation prompt.
/ #raw("--dry-run"): Report what a real deploy would upload and remove, without
  writing anything to the destination.

=== announce

Announce the site's metadata to the configured destination: an
atproto/#link("https://standard.site")[standard.site] publication plus one
document record per dated page. It announces the site; it does not upload the
built files (see #link("deploy/overview.typ")[Deploying] for that). Builds first, then
reconciles the remote records with your pages.

/ #raw("--password <pw>"): `-` reads it from stdin; prefer that or the
  environment variable over a literal flag.
/ #raw("-y, --yes"): Skip the confirmation prompt.
/ #raw("--dry-run"): Report what a real announce would send and remove, without
  writing. Needs no password: it diffs against the live repository over public
  reads.

=== init \[dir\]

Scaffold a whole project (config, templates, a starter page, and a stylesheet)
into `dir`, or the current directory. It prompts for the site name, author
(defaulted from your git config), and base URL, then offers to set up version
control with a `.gitignore`.

Four starter shapes ship with the binary, selected with `-t`:

#table(
  columns: 2,
  align: (left, left),
  table.header([Template], [What you get]),
  [`blog` (default)], [Dated posts, tags, pagination and feeds.],
  [`docs`], [Ordered sections, sidebar nav and client-side search.],
  [`book`], [Ordered chapters, also exported as one HTML file.],
  [`minimal`], [One page and one template, nothing else.],
)

/ #raw("-t, --template <name>"): Starter shape to scaffold. Defaults to `blog`.
/ #raw("--with <feature,..>"): Switch on optional features, appending their
  config blocks: `spa` and `standalone`
  (#link("../features/build/navigating.typ")[client-side navigation and
  single-file export]), `speculation` (prefetch hints), and `search`
  (#link("../features/discovery/search.typ")[a client-side search index]).
/ #raw("--theme <spec>"): Take templates and assets from a
  #link("../features/build/themes.typ")[theme package] instead of scaffolding
  copies of them. Only the shape's content and config are written.
/ #raw("--no-sample"): Scaffold the shape without its example pages. The home
  page stays: a site with no content at all builds to nothing.
/ #raw("--title <text>"), #raw("--author <name>"), #raw("--url <url>"): Fill the
  config without being prompted.
/ #raw("--lang <code>"): Default language code (default `en`).
/ #raw("--vcs <git|jujutsu>"): Choose a VCS instead of being prompted.
/ #raw("-y"): Accept every default non-interactively.

Existing files are never overwritten: `init` in a populated directory skips what
is already there and reports it.

=== clean

Remove build output and local build state. With no flag it sweeps everything:
the output plus the `.baudelaire` scratch root (cache and announce state). The
flags narrow it, so `clean --cache` forces a rebuild without discarding announce
state.

/ #raw("--dist"): Remove only the output directory.
/ #raw("--cache"): Remove only the build cache.
/ #raw("--announce"): Remove only the announce state.

This is the wholesale wipe, and it is not the config's
#link("config.typ")[`prune`], which sweeps only files no page claims any more
and runs as part of every build.

== Global flags <global-flags>

Accepted by every command.

/ #raw("--config <path>"): Config file to read (default `config.kdl`).
/ #raw("--root <dir>"): Change into `dir` first, so every relative path
  resolves under it.
/ #raw("--profile <name>"): Apply a named profile from the `profiles` block.
/ #raw("-v"), #raw("-q"): More (`-v`, repeat for deeper logs) or less output.
/ #raw("--strict"): Fail the run if anything warned. Warnings are what
  baudelaire says instead of failing (a missing font, an untaken permalink, a
  capability this binary lacks), and this is what turns the whole set into an
  exit code rather than something CI has to grep stderr for. The warnings still
  print; the run then exits non-zero behind them.

== Build flags <build-flags>

Config overrides accepted by the commands that build: `build`, `serve`, and
`check`.

/ #raw("--out <dir>"): Override the output directory.
/ #raw("--base-url <url>"): Override the site URL, useful for preview deploys.
/ #raw("--drafts"), #raw("--future"): Include draft or future-dated pages.
/ #raw("--cache"): Use the incremental cache (the default).
/ #raw("--strict-links"): Treat broken internal `.typ` links as errors (the
  default).

Every boolean flag has a `--no-` counterpart: `--no-drafts`, `--no-future`,
`--no-cache`, `--no-strict-links`. The pair matters because config can set
either side, so a setting turned on in `config.kdl` needs a way back off for one
run: `draft { build #true }` plus `--no-drafts` is a production build from a
config that normally includes drafts. Passing both, the last one wins.
