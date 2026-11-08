#let frontmatter = (
  title: "theme",
  order: 2,
)
#import "/templates/theme.typ": callout

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

== Where a theme comes from <sources>

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

== Where the copy goes

Every verb looks where your `config.kdl` says the theme is: a `theme
"vendor/albatros"` line is where `list`, `info`, `update` and `remove` find that
theme. Without a line naming it, they use `themes/<name>`, which is where `add`
writes one.

`--dir <path>` overrides both, as long as it stays inside the project root,
which is as far as a Typst import can reach.

== What is yours <lock>

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

See #link("../../theming/using.typ")[themes] for what each one is, and
#link("../../theming/authoring.typ")[writing a theme] for making your own.

#callout(kind: "note")[
  This command only exists in a build with the `themes` feature. A `slim` binary
  carries no themes; a theme there is a directory you put in the project
  yourself.
]
