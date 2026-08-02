#let frontmatter = (
  title: "Dev server & preview",
  order: 5,
)
#import "/templates/theme.typ": callout

A local server that rebuilds on save, reloads the browser, and can open the typst
line behind anything you alt-click.

```sh
$ baudelaire serve
```

Builds the site, serves it on `http://127.0.0.1:1821`, opens a browser, and
watches for changes.

== Configuration

```kdl
serve {
  port 3000
  bind "127.0.0.1"
  open #false
  include "data/**/*.json"
  exclude "assets/style.css"
}
```

#table(
  columns: 4,
  align: (left, left, left, left),
  table.header([Key], [Type], [Default], [Does]),
  [`port`],
  [number],
  [`1821`],
  [The port it listens on.],

  [`bind`],
  [str],
  [`127.0.0.1`],
  [The address it binds.],

  [`open`],
  [flag],
  [`#true`],
  [Opens a browser when the server starts.],

  [`watch`],
  [flag],
  [`#true`],
  [Watches the sources and rebuilds. Off, it serves what is already built.],

  [`include`],
  [str ..],
  [--],
  [Extra paths to watch, as globs, one word each.],

  [`exclude`],
  [str ..],
  [--],
  [Paths to leave unwatched, one word each.],

  [`editor`],
  [str ..],
  [--],
  [The command an alt-click runs, program and arguments as separate words.],
)

`--port`, `--bind`, `--no-open` and `--no-watch` override the config for one run.

#callout(kind: "warn")[
  The dev server has no authentication. `bind` defaults to loopback for that
  reason. Binding `0.0.0.0` puts your unfinished drafts on the local network.
]

== Watching

The content, template, asset and static trees are watched by default, plus
`config.kdl` itself: editing it reloads the config live and rebuilds the watcher,
so a change to `paths` or `include` takes effect without a restart. A `port` or
`bind` change does need one, since the server is already bound.

Globs are #link("https://docs.rs/wax")[wax] patterns relative to the project
root, and `exclude` wins over `include`.

```kdl
serve {
  include "data/**/*.json"
  exclude "assets/style.css"
}
```

A #link("hooks.typ")[hook] that writes into a watched directory needs its output
excluded, or the watcher retriggers itself forever.

Rebuilds are #link("incremental.typ")[incremental], so a save is usually a
handful of milliseconds. A failed rebuild doesn't take the server down: the
diagnostic is pushed into every open tab as an overlay, and the last good page
stays on screen.

== Live reload

Serving HTML injects a small script that opens a server-sent-event stream on
`/__baudelaire/live`. A successful rebuild pushes a reload; a failed one pushes
the error. Nothing is injected into a `--no-watch` session, and nothing is ever
injected into a `build`.

== Alt-click to source

```sh
$ baudelaire serve --spans
```

`--spans` is `html { spans #true }`. It stamps every element the author wrote
with the file, line and column that produced it:

```html
<main data-typst="templates/page.typ:2:3">
  <h2 id="a-heading" data-typst="content/note.typ:3:1">A heading</h2>
  <p data-typst="content/note.typ:5:1">A paragraph with
    <em data-typst="content/note.typ:5:19">emphasis</em>.</p>
</main>
```

The location comes from the compiler, so an inline `#emph` names its own column,
and what a layout emitted names the *template* rather than the page it was
rendering. Paths are project-relative; lines and columns are one-based.

Elements baudelaire synthesizes carry no stamp, because nobody wrote them: the
meta tags, an inlined icon body, the speculation rules. Neither does anything
from a typst package, whose files live in a download cache.

Then name the command that opens a location:

```kdl
serve {
  editor "code" "--goto" "{file}:{line}:{column}"
}
```

Alt-click any element and the line that wrote it opens. Other editors, one line
each:

```kdl
serve { editor "zed" "{file}:{line}:{column}" }
serve { editor "idea" "--line" "{line}" "{file}" }
serve { editor "nvim" "--server" "/tmp/nvim" "--remote-send" ":e +{line} {file}<CR>" }
```

The program and each argument are their own word. `{file}`, `{line}` and
`{column}` are substituted per argument, so a value lands inside whatever shape
your editor wants and never splits in two. The nearest stamped ancestor wins, so
clicking a word inside a paragraph asks for the paragraph. A link is not
followed: the alt-click was for the editor.

#callout(kind: "note")[
  There is no default editor. With none set, an alt-click says so in the page
  instead of guessing at `$EDITOR`.
]

The endpoint that opens a file exists only while `serve` is watching, only
answers the page it served itself, and only opens files inside the project. The
command is yours, from your config, and runs directly rather than through a
shell, so a path is an argument and can never become a second command.

#callout(kind: "warn")[
  The stamps are markup, so the build cache keys on them: switching `spans` on or
  off rebuilds the site once. Keep them out of a published build. An attribute on
  every element is for whoever writes the page, not whoever reads it.
]
