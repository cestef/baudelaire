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
/ #raw("-e, --edit"): Open the new file in `$EDITOR`. (`--open` still works;
  `serve --open` opens a browser, which is a different thing.)

=== deploy

Build the site, then upload the output to the destination in the `deploy` config
block: an #link("deploy/s3.typ")[S3-compatible bucket] or an
#link("deploy/ssh.typ")[SSH/SFTP host]. It uploads the built files; it does not
announce metadata (see #link("deploy/overview.typ")[Deploying] for the full
picture). Only changed files are sent, and stale remote files are pruned unless
`delete` is off. Errors if no `deploy` block is configured.

Because it builds, it accepts the #link(<build-flags>)[build flags]: deploy a
preview with `--base-url`, a staging copy with `--drafts`, or a cold artifact
with `--no-cache`.

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
reconciles the remote records with your pages. Accepts the
#link(<build-flags>)[build flags], like `deploy`.

/ #raw("--secret <pw>"): The app password. `-` reads it from stdin; prefer that
  or the environment variable over a literal flag. Spelled the same as
  `deploy`'s, and `--password` still works.
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
/ #raw("--vcs <git|jujutsu>"): Set up this VCS without being prompted for one.
  Naming it is the only way to get a repository without a prompt, so
  `baudelaire init -y --vcs git` is the scripted spelling.
/ #raw("-y"): Take the default answer to every prompt instead of asking. On its
  own it sets up no version control: a run that only wanted to silence the
  prompts should not leave a repository behind.

Existing files are never overwritten: `init` in a populated directory skips what
is already there and reports it.

The global `--config` names the file to write, so `init --config site.kdl`
scaffolds a project whose config every later command finds under the same flag.
It has to be a bare filename: a `paths { }` entry resolves against the working
directory, not against the config file, so a config nested a directory down
would name a content tree outside its own project. `--profile` is refused
outright, having nothing to narrow in a project that does not exist yet.

=== clean

Remove build output and local build state. With no flag it sweeps everything:
the output plus the `.baudelaire` scratch root (cache and announce state). The
flags narrow it, so `clean --cache` forces a rebuild without discarding announce
state.

Every directory it is about to remove is printed first, and the wholesale sweep
asks before going ahead: it takes announce state with it, which is what the next
`announce` reconciles a live repository against. A narrowed sweep does not ask.
Off a terminal the sweep stops rather than answering for itself, so pass `--yes`
in CI.

/ #raw("--all"): Remove everything. The same as passing no flag, said out loud,
  so a script can distinguish meaning it from forgetting a flag.
/ #raw("--output"): Remove only the output directory (`--dist` still works).
/ #raw("--cache"): Remove only the build cache.
/ #raw("--announce"): Remove only the announce state.
/ #raw("-y, --yes"): Skip the confirmation.
/ #raw("--dry-run"): Print what would be removed and remove nothing.

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
/ #raw("--json"): Write a machine-readable summary of the run to stdout, one
  JSON object, on its own. Everything else baudelaire prints goes to stderr, so
  `baudelaire --json build 2>/dev/null | jq` is safe:

  ```json
  {
    "ok": true,
    "pages": 51,
    "cached": 50,
    "warnings": 1,
    "diagnostics": [
      { "code": "baudelaire::links::broken",
        "severity": "warning",
        "message": "found 1 broken internal link" }
    ]
  }
  ```

  `pages`/`cached` are absent for a command that builds nothing. `ok` is false
  whenever the run failed, `--strict` included.
/ #raw("-V"), #raw("--version"): `-V` is the one line a script greps
  (`baudelaire 0.0.7`); `--version` reports the whole build:

  ```
  baudelaire 0.0.7
    commit    465556976431
    rustc     1.97.1 (8bab26f4f 2026-07-14) (release)
    target    x86_64-unknown-linux-gnu
    flavor    full
    features  announce cards css embedded-fonts images js ssh
  ```

  `flavor` names the published build this matches, spelled the way
  #link("install.typ")[`install.sh`] spells it: `full`, `slim`, or `custom` for a
  feature mix no release ships. `features` is what this binary can do, and a
  build missing any of them gains a `without` row naming them, which is the
  answer to "why is my `assets { bundle }` doing nothing". A `-dirty` suffix on
  `commit` means the tree it was built from had uncommitted changes, so the
  commit alone will not reproduce it.
/ #raw("--strict"): Fail the run if anything warned. Warnings are what
  baudelaire says instead of failing (a missing font, an untaken permalink, a
  capability this binary lacks), and this is what turns the whole set into an
  exit code rather than something CI has to grep stderr for. The warnings still
  print; the run then exits non-zero behind them.

== Build flags <build-flags>

Config overrides accepted by the commands that build: `build`, `serve`, `check`,
and the two that build before publishing, `deploy` and `announce`. (`check`
writes no file and loads no cache, so it takes neither `--out` nor `--cache`.)

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
