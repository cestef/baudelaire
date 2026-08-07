#let frontmatter = (
  title: "CLI",
  order: 1,
)
#import "/templates/theme.typ": callout

Every command reads `config.kdl` from the working directory and takes the
#link(<global>)[global flags].

```sh
baudelaire                  # build (the default command)
baudelaire serve --open     # dev server, live reload
baudelaire new posts/hello  # scaffold content/posts/hello.typ
```

Every command has a visible short alias, so `baudelaire b` builds and
`baudelaire s` serves.

#table(
  columns: 3,
  align: (left, left, left),
  table.header([Command], [Alias], [Does]),
  [`build`], [`b`], [Compile the site into the output directory.],
  [`serve`], [`s`], [Build, serve, watch, live-reload.],
  [`check`], [`c`], [Compile and check links, writing nothing.],
  [`new <path>`], [`n`], [Scaffold one content file.],
  [`deploy`], [`d`], [Build, then upload to the configured destination.],
  [`announce`], [`a`], [Build, then announce the site's metadata.],
  [`init [dir]`], [`i`], [Scaffold a whole project.],
  [`clean`], [`cl`], [Remove build output and local build state.],
  [`completions <shell>`], [`comp`], [Print a shell completion script to stdout.],
  [`man`], [--], [Print the manual as roff to stdout.],
  [`reference [key]`], [`ref`], [Print every config key and its value shape.],
  [`mirror`], [`packages`, `pkg`], [Write the generated modules to disk for editor tooling.],
  [`theme <verb>`], [`th`], [List, add, inspect, update or remove a shipped theme.],
)

#callout(kind: "warn")[
  `check` takes `c` and `clean` takes `cl`, not the other way around. One
  keystroke should not be the difference between compiling and deleting.
]

== Global flags <global>

#table(
  columns: 3,
  align: (left, left, left),
  table.header([Flag], [Default], [Does]),
  [`-c, --config <path>`], [`config.kdl`], [Config file to read.],
  [`-r, --root <dir>`], [--], [Change into `dir` first, so relative paths resolve under it.],
  [`-p, --profile <name>`], [--], [Apply a named #link("../configure/profiles.typ")[profile].],
  [`--theme <theme>`], [--], [Build with this #link("../start/themes.typ")[theme], whatever `theme` in the config says. A profile cannot name one: the theme's own defaults are read before profiles are overlaid.],
  [`-v, --verbose`], [--], [Per-page progress plus debug logs (`-vv` for trace).],
  [`-q, --quiet`], [--], [Less output; `-qq` leaves only diagnostics and the exit code. Conflicts with `-v`.],
  [`--color <when>`], [`auto`], [`auto`, `always` or `never`. Beats every environment signal, including `NO_COLOR` and `CLICOLOR_FORCE`.],
  [`--strict`], [--], [Fail the run if anything warned.],
  [`--json`], [--], [Write one machine-readable summary object to stdout.],
  [`-V`], [--], [Print one line: `baudelaire 0.0.11`.],
  [`--version`], [--], [Print the full build report.],
  [`-h, --help`], [--], [Print help for the command.],
)

Everything baudelaire prints goes to stderr, and the `--json` object is the only
thing a build puts on stdout, so `baudelaire --json build 2>/dev/null | jq` is
safe:

```json
{
  "schema": 1,
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

`pages` and `cached` are absent for a command that builds nothing. `ok` is false
whenever the run failed, `--strict` included. `schema` moves only when a field
changes meaning or type or goes away, never when one is added.

`--version` reports what this binary can do:

```text
baudelaire 0.0.11
  commit    465556976431
  rustc     1.97.1 (8bab26f4f 2026-07-14) (release)
  target    x86_64-unknown-linux-gnu
  flavor    full
  features  announce cards css embedded-fonts images js markdown pdf ssh themes
```

`flavor` is `full`, `slim`, or `custom` for a feature mix no release ships,
spelled the way #link("../start/install.typ")[the installer] spells it. A build
missing a feature gains a `without` row naming what it lacks, which is the answer
to "why is my `assets { bundle }` doing nothing". A `-dirty` suffix on `commit`
means the tree had uncommitted changes.

== Build flags <build-flags>

Config overrides taken by every command that builds: `build`, `serve`, `check`,
`deploy`, `announce`.

#table(
  columns: 2,
  align: (left, left),
  table.header([Flag], [Does]),
  [`-o, --out <dir>`], [Override the output directory. Not on `check`.],
  [`--base-url <url>`], [Override the site URL. Useful for preview deploys.],
  [`--drafts`], [Build draft pages.],
  [`--future`], [Build future-dated pages.],
  [`--cache`], [Use the incremental cache (on by default). Not on `check`.],
  [`--strict-links`], [Error on broken internal links (on by default).],
)

Every boolean here has a `--no-` counterpart: `--no-drafts`, `--no-future`,
`--no-cache`, `--no-strict-links`. The pair exists because config can set either
side, so `drafts #true` plus `--no-drafts` is a production build from a
config that normally includes drafts. Pass both and the last one wins.

`check` writes no file and loads no cache, so it takes neither `--out` nor
`--cache`.

== build

Compile the site into the output directory, incrementally by default. The default
command, so bare `baudelaire` builds. Takes the
#link(<build-flags>)[build flags] and nothing else.

```sh
baudelaire --profile prod build
```

== serve

Build, serve over HTTP, watch the sources, and live-reload the browser. Takes the
#link(<build-flags>)[build flags], plus:

#table(
  columns: 3,
  align: (left, left, left),
  table.header([Flag], [Default], [Does]),
  [`--port <n>`], [`1821`], [Port to listen on.],
  [`--bind <addr>`], [`127.0.0.1`], [Address to bind.],
  [`--open`], [on], [Open a browser on start (`--no-open` suppresses it).],
  [`--watch`], [on], [Rebuild on change (`--no-watch` serves statically).],
  [`--spans`], [off], [Stamp each element with the source it came from.],
)

`--spans` sets `html { spans }`, and alt-clicking the preview then opens that
line in `serve { editor }`. See #link("../build/preview.typ")[the dev server].

#callout(kind: "note")[
  `--spans` is markup, and the cache keys on markup, so turning it on rebuilds
  the site once.
]

== check

Compile every page and report broken internal links without writing output. A
fast CI gate. Takes the #link(<build-flags>)[build flags] except `--out` and
`--cache`, plus:

#table(
  columns: 2,
  align: (left, left),
  table.header([Flag], [Does]),
  [`--external`], [Also verify outbound `http(s)` links over the network.],
  [`--no-external`], [Skip them even when `links { external }` is set.],
)

Builds never reach the network, whatever `links { external }` says. See
#link("../build/linting.typ")[linting] for what the check covers and how to tune
its deadline, its cache and what it may skip.

== new

```sh
baudelaire new posts/hello
```

Scaffolds `content/posts/hello.typ`, inferring the title from the filename, the
date or `order` from the collection's sort key, and the template and permalink
the page will occupy. A bare name lands under the content directory; an explicit
`content/posts/hello.typ` is not double-prefixed.

#table(
  columns: 3,
  align: (left, left, left),
  table.header([Flag], [Default], [Does]),
  [`--title <text>`], [from filename], [Page title.],
  [`--date <YYYY-MM-DD>`], [today], [Publication date, for dated collections.],
  [`--draft`], [on], [Mark the page a draft (`--no-draft` publishes it).],
  [`-b, --bundle`], [off], [Create `<name>/index.typ` for colocated assets.],
  [`-e, --edit`], [off], [Open the new file in `$EDITOR`.],
)

Drafting is the default: a page being written is not a page being published.
`--open` still works as an alias of `-e`, but `serve --open` opens a browser,
which is a different thing.

== deploy

Build, then upload the output to the destination in the `deploy` block: an
#link("../ship/hosts/s3.typ")[S3-compatible bucket] or an
#link("../ship/hosts/ssh.typ")[SSH/SFTP host]. Only changed files are sent. Errors when
no `deploy` block is configured. Takes the #link(<build-flags>)[build flags],
plus:

#table(
  columns: 2,
  align: (left, left),
  table.header([Flag], [Does]),
  [`--secret <value>`], [S3 secret key or SSH password/passphrase. `-` reads it from stdin.],
  [`-y, --yes`], [Skip the confirmation prompt.],
  [`--dry-run`], [Report what would change without writing to the destination.],
)

Prefer stdin, the backend's environment variable, or the interactive prompt: a
literal flag can leak into shell history. See
#link("../ship/deploy.typ")[deploying].

== announce

Build, then announce the site's metadata to the configured destination: an
atproto publication plus one record per dated page. It publishes metadata, not
files. Takes the #link(<build-flags>)[build flags], plus:

#table(
  columns: 2,
  align: (left, left),
  table.header([Flag], [Does]),
  [`--secret <value>`], [App password or token. `-` reads it from stdin.],
  [`-y, --yes`], [Skip the confirmation prompt.],
  [`--dry-run`], [Report what would be sent and removed, without writing.],
)

`--password` still works as an alias of `--secret`. `--dry-run` needs no secret:
it diffs against the live repository over public reads. See
#link("../ship/announce.typ")[announcing].

#callout(kind: "note")[
  This command only exists in a build with the `announce` feature. A `slim`
  binary has no `announce`, and its completion script offers none.
]

== init

Scaffold a whole project (config, templates, a starter page, a stylesheet) into
`dir`, or the current directory. Prompts for the site name, author, and base URL,
then offers to set up version control. With `--theme`, the templates and the
stylesheet come from the theme instead, and the run closes on where to put it.

#table(
  columns: 3,
  align: (left, left, left),
  table.header([Flag], [Default], [Does]),
  [`-t, --template <name>`], [`blog`], [Starter shape. Unused with `--theme`.],
  [`--with <feature,..>`], [--], [Switch on optional features. One the shape already configures is skipped.],
  [`--theme <spec>`], [--], [Scaffold against a #link("../start/themes.typ")[theme]: identity, paths and a preview, and nothing the theme declares. A spec naming a shipped theme also writes it.],
  [`--title <text>`], [prompted], [Site title.],
  [`--author <name>`], [prompted], [Site author, defaulted from git `user.name`.],
  [`--url <url>`], [prompted], [Canonical base URL.],
  [`--lang <code>`], [`en`], [Default language code.],
  [`--no-sample`], [off], [Scaffold the shape without its example pages.],
  [`--vcs <git|jujutsu>`], [prompted], [Set up this VCS without asking (`jj` works too).],
  [`-y, --yes`], [off], [Take the default answer to every prompt.],
)

The four starter shapes:

#table(
  columns: 2,
  align: (left, left),
  table.header([Template], [What you get]),
  [`blog`], [Dated posts, tags, pagination and feeds.],
  [`docs`], [Ordered sections, sidebar nav and client-side search.],
  [`book`], [Ordered chapters, also exported as one HTML file.],
  [`minimal`], [One page and one template, nothing else.],
)

`--with` takes a comma-separated list:

#table(
  columns: 2,
  align: (left, left),
  table.header([Feature], [Appends]),
  [`spa`], [`navigation { spa { } }`, #link("../ship/navigating.typ")[client-side navigation].],
  [`standalone`], [`navigation { standalone { } }`, a single-file export.],
  [`speculation`], [`navigation { speculation { } }`, browser prefetch hints.],
  [`search`], [`generate { search { formats "json" } }`, a #link("../build/generate/search.typ")[client-side index].],
  [`pdf`], [`generate { pdf { pages { .. } } }`, a #link("../build/generate/pdf.typ")[PDF per page] from `print.typ`.],
)

Existing files are never overwritten: `init` in a populated directory skips what
is there and reports it. Every scaffold writes a `.gitignore` for `public/` and
`.baudelaire/`, whether or not it sets up a repository; `-y` on its own sets one
up for no VCS, so name `--vcs git` for the scripted spelling.

#callout(kind: "warn")[
  `--config` names the file `init` *writes*, and it has to be a bare filename: a
  `paths { }` entry resolves against the working directory, not against the
  config file. `--profile` is refused outright, having nothing to narrow in a
  project that does not exist yet.
]

== clean

Remove build output and local build state. With no flag it sweeps everything: the
output directory plus the `.baudelaire` scratch root.

#table(
  columns: 2,
  align: (left, left),
  table.header([Flag], [Does]),
  [`--all`], [Remove everything. The same as no flag, said out loud.],
  [`--output`], [Remove the output directory only (`--dist` still works).],
  [`--cache`], [Remove the #link("../build/incremental.typ")[incremental cache] only.],
  [`--announce`], [Remove local announce state only.],
  [`-y, --yes`], [Skip the confirmation prompt.],
  [`--dry-run`], [Print what would be removed and remove nothing.],
)

Every directory is printed before anything is removed, because the paths come
from config. The wholesale sweep asks first, since it takes announce state with
it; a narrowed `clean --cache` does not. Off a terminal the full sweep stops
rather than answering for itself, so pass `-y` in CI. A target that would swallow
the project (`paths { dist "." }`) is refused.

`clean` sweeps project state only. The `@baudelaire/*` modules installed for
editor tooling are machine-global and no config locates them, so they are
`baudelaire mirror --uninstall`, below.

This is not the config's `prune`, which sweeps only files no page claims and runs
as part of every build.

== theme

```sh
baudelaire theme list             # the four shipped themes, and what you have installed
baudelaire theme add albatros     # writes themes/albatros/
baudelaire theme info albatros    # what it declares, and what your copy has become
baudelaire theme update albatros  # rewrite it from where it came from, keeping your edits
baudelaire theme remove albatros  # take it back off
```

#table(
  columns: 3,
  align: (left, left, left),
  table.header([Command], [Alias], [Does]),
  [`list`], [`ls`], [The shipped themes, then the ones you installed from anywhere else, each marked with where it is, where it came from, and how many files you have edited.],
  [`add <spec>`], [--], [Fetch a theme and write it into the project, then print the `config.kdl` line to add. Files already there are kept.],
  [`info <name>`], [--], [Its templates, the collections and taxonomies its `theme.kdl` declares, where it came from, and the state of your copy.],
  [`update <name>`], [`up`], [Fetch it again from the source its record names, and rewrite the files you have not touched.],
  [`remove <name>`], [`rm`, `uninstall`], [Delete the files still baudelaire's.],
)

=== Where a theme comes from <sources>

`add` takes a spec, and what it looks like decides where the theme is fetched
from. Every one of them ends the same way: the files land in your project, are
yours from that moment, and carry a record of where they came from so `update`
can go back without being told again.

#table(
  columns: 2,
  align: (left, left),
  table.header([Spec], [Fetched from]),
  [`albatros`], [The binary. One of the four `theme list` prints, and no network.],
  [`./plume`, `/srv/themes/plume`, `~/plume`], [A directory on this machine, copied in.],
  [`@preview/plume:1.0.0`], [The Typst package store, the same one the compiler resolves an `#import` through. Copying it makes it yours to edit, which is the difference from naming the package in `theme`.],
  [`gh:owner/plume`, `gl:`, `cb:`, `sr:`], [A repository at GitHub, GitLab, Codeberg or sourcehut, fetched as that forge's own source archive.],
  [`https://github.com/owner/plume`, `https://codeberg.org/…`], [The same, named by its URL.],
  [`forgejo:git.example.net/owner/plume`, `gitea:`, `gitlab:`, `github:`], [A self-hosted instance of that software, at the host you name.],
  [`https://…/plume-1.0.0.tar.gz`, `…​.tgz`, `…​.zip`], [An archive over http, which is what a forge's source download is.],
)

A repository spec takes an optional `#ref`, naming a tag, a branch or a commit;
without one it fetches `HEAD`. The record keeps the ref, which is what a later
`update` follows, and what the forge actually served: the commit for a tag or a
commit, the branch's own name for a branch.

```sh
baudelaire theme add gh:owner/plume#v1.2.0
```

Nothing is cloned. A forge's source archive of one revision is the same files in
one request, and a repository named any other way (an ssh remote, a host this
does not know) is fetched by naming its archive URL directly.

`--subdir <path>` names the directory *inside* a repository or an archive that
holds the theme, for a project that carries more than one. The copy is called
after that directory, and the record keeps it, so `update` goes back to the same
place.

```sh
baudelaire theme add gh:cestef/baudelaire --subdir themes/albatros
```

Only these two spellings reach the network: a repository and an archive. The
shelf, a directory and the package store do not, and no other theme verb does.

=== Where the copy goes

Every verb looks where your `config.kdl` says the theme is: a `theme
"vendor/albatros"` line is where `list`, `info`, `update` and `remove` find that
theme. Without a line naming it, they use `themes/<name>`, which is where `add`
writes one.

`--dir <path>` overrides both, as long as it stays inside the project root,
which is as far as a Typst import can reach.

=== What is yours <lock>

A theme is *vendored*: its files are in your project, committed with it, and
yours to edit. So `add` leaves a `.baudelaire-lock.json` beside them, recording
which theme it is, which baudelaire wrote it, and what each file digests to in
that baudelaire. That record is the whole of baudelaire's package state, and it
exists to answer one question: which of these files are still ours?

#table(
  columns: 2,
  align: (left, left),
  table.header([Your copy of a file], [`update` / `remove` does]),
  [Untouched since it was written], [Rewrites it / deletes it.],
  [Edited], [Leaves it and says so. `--force` overrides.],
  [Deleted], [Leaves it deleted: an update that puts a file back undoes a decision.],
  [Already there when `add` ran], [Never recorded, so `update` leaves it (`--force` overrides) and `remove` never deletes it, at any force.],
)

A run records only the files it wrote, so `add` over an install from an earlier
baudelaire changes nothing about the files that binary left: they are still its,
and still what `update` brings forward.

A `remove` that kept something keeps the record with it, so the same command
with `--force` can still finish.

Every verb names one of the four shipped themes, so a theme of your own is
outside all of this: there is nothing to update it from. A copy of a shipped one
made by hand has no record either, so it is entirely yours, and `update` writes
over it only with `--force`.

#callout(kind: "note")[
  There is no lockfile for anything else, and nothing to resolve: a vendored
  theme is files in your repository, and a theme you *name* in `theme` rather
  than install (`@namespace/name:1.0.0`) is pinned by the exact version in its
  spec, resolved through Typst's own package store.
]

See #link("../start/themes.typ")[themes] for what each one is, and
#link("../write/theme-authoring.typ")[writing a theme] for making your own.

#callout(kind: "note")[
  This command only exists in a build with the `themes` feature. A `slim` binary
  carries no themes; a theme there is a directory you put in the project
  yourself.
]

== completions

```sh
baudelaire completions fish > ~/.config/fish/completions/baudelaire.fish
```

Prints a completion script for `bash`, `elvish`, `fish`, `nushell`, `powershell`
or `zsh` to stdout, and nothing else, so it can be redirected straight into a
completion directory. `baudelaire completions --help` prints the line for every
shell, including where each expects the file.

The script is generated from the same definition the binary parses with, so it
offers exactly the commands and flags this build has.

== man

```sh
baudelaire man > ~/.local/share/man/man1/baudelaire.1
```

Prints the manual as roff to stdout. Same definition as `--help` and the
completion scripts, so the three cannot disagree.

== reference

```sh
baudelaire reference                # every key
baudelaire reference assets.images  # that block and below
```

Prints every key `config.kdl` accepts, with its value shape, as an indented tree.
Reads no config: it describes the schema, not your site. A dotted key narrows the
output, which is usually what you want, since the whole schema is over a hundred
and fifty keys. An unknown key is an error suggesting the nearest real one.

Same data as the #link("../configure/reference.typ")[config reference] page,
printed from the binary you have.

== mirror

```sh
baudelaire mirror              # into .baudelaire/generated/
baudelaire mirror --global     # typst packages into typst's own directory
baudelaire mirror --uninstall  # take it all back off
```

Writes every generated module where an editor resolves it: the
#link("typst-modules.typ")[`@baudelaire/*` modules] as ordinary typst packages
under `.baudelaire/generated/packages/`, and the
#link("js-modules.typ")[`baudelaire:*` modules] as
`.baudelaire/generated/baudelaire.d.ts`. `init` runs it for you; run it again
after upgrading baudelaire.

The run closes on the settings it still needs, ready to paste. `-v` lists every
module it wrote.

```
◆ editor setup
➜ typst     --package-path /abs/path/.baudelaire/generated/packages
  ↳ or TYPST_PACKAGE_PATH; tinymist takes it in typstExtraArgs
➜ tsconfig  add .baudelaire/generated/baudelaire.d.ts to the include list
```

#table(
  columns: 2,
  align: (left, left),
  table.header([Flag], [Does]),
  [`--global`], [Put the typst packages in typst's own package directory, where they resolve with nothing configured and every project shares one copy.],
  [`--path DIR`], [Put the typst packages here (or remove them from here) instead.],
  [`--uninstall`], [Remove what a run wrote, and nothing else in those locations.],
)

Both families land in the project by default, because three of the four typst
modules describe *this* site: `site` from its config, `sections` and `pages`
from its pages. One shared copy of those shows one project's data to every other
project's editor. The price is that one typst setting; `--global` is the trade
that buys it back.

A project is optional: outside one, `sections` and `pages` are written empty. A
build never reads any of it, so a stale copy can't change a page, and every
build rewrites the declarations.

Uninstalling removes baudelaire's own namespace directory and its declaration
file only, so `@local` packages sitting beside them are untouched. A run made
with `--global` or `--path` comes off the same way, with the same flag.
