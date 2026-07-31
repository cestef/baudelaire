#let frontmatter = (
  order: 25,
  title: "Source-mapped preview",
  tags: ("feature", "typst", "workflow"),
  summary: "Every element remembers the line that wrote it, so alt-clicking the preview opens that line in your editor.",
)
#import "/templates/theme.typ": callout

There is a real compiler under this site, and a compiler knows where everything
came from. Typst carries a span on every node it emits, Baudelaire post-processes
pages as a typed DOM rather than as text, so the span survives all the way to the
markup: each element can say which file, line and column produced it.

```sh
$ baudelaire serve --spans
```

Alt-click anything on the page and the line that wrote it opens in your editor.

== The stamps

`--spans` is `html { spans #true }`, and it stamps every element the author
wrote:

```html
<main data-typst="templates/page.typ:2:3">
  <h2 id="a-heading" data-typst="content/note.typ:3:1">A heading</h2>
  <p data-typst="content/note.typ:5:1">A paragraph with
    <em data-typst="content/note.typ:5:19">emphasis</em>.</p>
</main>
```

The location is the compiler's, not a guess: an inline `#emph` names its own
column, and what a layout emitted names the *template* rather than the page it
was rendering. Paths are relative to the project root, and lines and columns are
one-based, the way an editor counts them.

Elements Baudelaire synthesizes carry none, because they were not written
anywhere: the meta tags, the inlined body of an icon, the speculation rules.
Neither does anything that came from a package, whose files live in a download
cache rather than in your site.

== Opening the source

Name the command that opens a location and the dev server runs it when you
alt-click:

```kdl
serve {
  editor "code" "--goto" "{file}:{line}:{column}"
}
```

The program and each of its arguments are their own word. `{file}`, `{line}` and
`{column}` are substituted per argument, so a value lands inside whatever shape
your editor wants and never splits in two:

```kdl
serve { editor "code" "--goto" "{file}:{line}:{column}" }      // VS Code
serve { editor "zed" "{file}:{line}:{column}" }                // Zed
serve { editor "nvim" "--server" "/tmp/nvim" "--remote-send" "<C-\><C-N>:e +{line} {file}<CR>" }
serve { editor "idea" "--line" "{line}" "{file}" }             // IntelliJ
```

The nearest stamped ancestor wins, so clicking a word inside a paragraph asks
for the paragraph when the word was not an element of its own. A link is not
followed: the alt-click was for the editor.

#callout(kind: "note")[
  There is no default. Launching a program is your instruction to give, and
  `$EDITOR` is as likely to name a terminal editor with no terminal to appear
  in. With no `editor` set, an alt-click says so in the page instead of guessing.
]

== What it will not do

The endpoint that opens a file exists only while `serve` is watching, only
answers the page it served itself, and only opens files inside the project. The
command is yours, from your config, and it is run directly rather than through a
shell, so a path is an argument and can never become a second command.

#callout(kind: "warning")[
  The stamps are markup, so the build cache keys on them: switching `spans` on
  or off rebuilds the site once. They are also off in a published build by
  design, since an attribute on every element is for whoever is writing the
  page, not for whoever is reading it.
]
