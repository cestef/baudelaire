#let frontmatter = (
  title: "Markdown to Typst",
  order: 8,
)
#import "/templates/theme.typ": callout

#callout(kind: "note")[
  *You may not need this page.* `.md` files are pages
  (#link("../../write/markdown.typ")[Markdown pages]), so the prose you already
  have can stay exactly as it is. What has to change is the *frontmatter*: YAML
  or TOML becomes a `---` block of KDL.

  ```md
  ---
  title "The night train"
  date "2019-03-02"
  tags "rail" "night"
  ---
  ```

  Convert to Typst when you want what Typst gives you -- math, a figure, a chart,
  a template helper mid-page. A markdown page can reach some of that through an
  `eval` fence without being converted at all.
]

If you do convert: Typst markup is close enough to Markdown that most prose
survives a mechanical pass, and different enough that the pass needs reading
afterwards.

== The mapping

#table(
  columns: 3,
  align: (left, left, left),
  table.header([Markdown], [Typst], [Note]),
  [`# Title`], [`= Title`], [One `=` per level.],
  [`## Section`], [`== Section`], [The usual top level here, since a template draws the title.],
  [`**bold**`], [`*bold*`], [One star, not two.],
  [`*italic*`, `_italic_`], [`_italic_`], [Underscores, not stars.],
  [an inline code span], [an inline code span], [Backticks, unchanged.],
  [a fenced block], [a fenced block], [Unchanged, and highlighted by the compiler.],
  [`[text](url)`], [`#link("url")[text]`], [A `.typ` target is rewritten to its permalink.],
  [`![alt](cover.png)`], [`#image("cover.png", alt: "alt")`], [Resolves beside the page.],
  [`- item`], [`- item`], [Unchanged.],
  [`1. item`], [`+ item`], [Typst numbers it for you.],
  [`> quote`], [`#quote(block: true)[quote]`], [],
  [`[^1]` and its note], [`#footnote[the note]`], [Written where it is referenced.],
  [a pipe table], [`#table(columns: 2, ..)`], [Cells are content, so `[..]` around anything but a literal.],
  [`---`], [`#h("hr")`], [From `@baudelaire/html`. Typst's `line` is a paged-layout element.],
  [`<div class="x">`], [`#h("div", class: "x")[..]`], [Raw HTML has no meaning in a Typst page.],
  [`<!-- comment -->`], [`// comment`], [`/* .. */` spans lines.],
  [`---` frontmatter], [`#let frontmatter = (..)`], [A Typst dict, so values are typed.],
)

Typst also has a term list, which Markdown never did:

```typ
/ Cold build: every page compiled.
/ Warm build: only what changed.
```

== Characters that mean something

`#`, `@`, `$`, `*`, `_`, `<` and the backtick are markup. In prose, escape the
one you meant literally:

```typ
Costs \$4, tagged \#rust, mail \@example.com.
```

`#` is the one that bites: it opens code, so `#1 in the charts` is a syntax error
rather than a sentence. A run of literal text is easier as `#raw("...")` or a
raw span than as a line of backslashes.

== A mechanical first pass

Pandoc writes Typst, which gets the prose, lists, tables, links and footnotes
across:

```sh
pandoc -f gfm+yaml_metadata_block -t typst --wrap=preserve post.md -o post.typ
```

Without `-s` the metadata block is consumed and dropped, leaving a body with no
frontmatter. To keep it, hand pandoc a template that writes the binding:

```text
// page.typ.template
#let frontmatter = (
  title: "$title$",
  date: $date$,
)

$body$
```

```sh
pandoc -f gfm+yaml_metadata_block -t typst -s \
  --template page.typ.template \
  --variable date="datetime(year: 2026, month: 7, day: 9)" \
  post.md -o post.typ
```

The date is the awkward one: `date` in a Markdown file is a string, and
frontmatter wants a `datetime(..)`. Either pass it per file as above, or convert
the strings in a second scripted pass.

Over a tree:

```sh
find content -name '*.md' | while read -r f; do
  pandoc -f gfm+yaml_metadata_block -t typst --wrap=preserve "$f" -o "${f%.md}.typ"
done
```

TOML frontmatter, which is what Zola writes, is not a metadata block to pandoc
and comes through as escaped text. Cut it first:

```sh
awk 'BEGIN{n=0} /^\+\+\+$/{n++; next} n>=2{print}' post.md | pandoc -f gfm -t typst -o post.typ
```

#callout(kind: "warn")[
  Read the output. Pandoc emits valid Typst, not idiomatic Typst: expect
  `#emph[..]` where `_.._` reads better, a `;` terminating a call before
  punctuation, `#figure(align(center)[#table(..)], kind: table)` around a plain
  table, and a `<label>` under every heading that duplicates what
  `html { anchors }` already emits.
]

Three things it drops or mangles, all of them silent:

#table(
  columns: 2,
  align: (left, left),
  table.header([In the Markdown], [What survives]),
  [a raw HTML block], [*nothing*. `<ul>`, `<div>`, an embedded widget: gone from the output.],
  [`{% post_url .. %}`, `{{< relref .. >}}`, `@/blog/x.md`], [escaped text, and no longer a link.],
  [`<!--more-->`], [nothing, along with the summary it used to mark.],
)

The first is the one to check for. A page whose interesting part was hand-written
HTML converts to an empty-looking page, and the build stays green because the
result is valid Typst. Grep the sources for `<` before you convert, and rewrite
those parts with `h()`.

== The parts pandoc cannot do

*Shortcodes.* Whatever spelling they had, they become Typst functions. Put them
in one file and import it:

```typ
// templates/parts.typ
#import "@baudelaire/html:0.1.0": h

#let aside(title, body) = h("aside", class: "note")[
  #h("strong", title)
  #body
]
```

```typ
#import "/templates/parts.typ": aside

#aside("Careful")[This is the shortcode you used to register.]
```

A shortcode that only wrapped markup is often not worth a function at all: write
the markup.

*Internal links.* Point at the source file, not the URL:

```typ
See #link("../start/quickstart.typ")[the quickstart].
```

The build rewrites it to that page's permalink and fails if there is no page
behind it, which is how a migration finds the links it broke. Old absolute URLs
(`/blog/hello/`) are passed through untouched, so they will not be checked and
will not follow a later rename. Convert them.

*Raw HTML.* Anything a Markdown file embedded verbatim is emitted with `h()`, and
the result goes through the same typed DOM as everything else, so the asset
rewriter and the linter see it.

*Images.* `#image("cover.png")` next to a page bundle
(`content/blog/hello/index.typ`) is the direct translation of a Markdown post
with its images beside it. It does not publish beside the page, though: the
pipeline writes it to `/assets/cover.png`, one flat namespace for the whole site.
A tree where every post directory holds its own `cover.png` therefore collides,
which the build reports as a warning per clash and resolves by serving one file
to every page. Turn on `assets { fingerprint }`, which names each by content, or
rename the sources. See #link("../../build/images.typ")[images] for what
the pipeline then does with it.

== Then

```sh
baudelaire check
```

Compiles every page and reports broken internal links without writing anything.
It is the fastest way to work through a converted tree: fix what it names, run it
again.
