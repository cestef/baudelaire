#let frontmatter = (
  title: "Markdown to Typst",
  order: 8,
)
#import "/templates/theme.typ": callout

#callout(kind: "note")[
  *Converting is optional.* `.md` files are pages
  (#link("../../write/markdown.typ")[Markdown pages]), frontmatter included: the
  fence picks the dialect, so `---` is YAML and `+++` is TOML, as in the
  original posts. A Markdown tree builds without any of the steps on this page.

  Convert a page to Typst when you want math, a figure, a chart or a template
  helper mid-page. A Markdown page can reach some of that through an `eval`
  fence without being converted.

  A Typst page can also keep one chunk as Markdown with
  #link("../../lookup/typst-modules.typ")[`md`], which suits a long table or
  reference list you would rather not translate by hand.
]

Most prose survives a mechanical conversion to Typst, but read the result
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
  [`[text](url)`], [`#link("url")[text]`], [A path naming a `.typ` or `.md` source is rewritten to its permalink.],
  [`![alt](cover.png)`], [`#image("cover.png", alt: "alt")`], [Resolves beside the page.],
  [`- item`], [`- item`], [Unchanged.],
  [`1. item`], [`+ item`], [Typst numbers it for you.],
  [`> quote`], [`#quote(block: true)[quote]`], [],
  [`[^1]` and its note], [`#footnote[the note]`], [Written where it is referenced.],
  [a pipe table], [`#table(columns: 2, ..)`], [Cells are content, so `[..]` around anything but a literal.],
  [`---`], [`#h("hr")`], [From `/.baudelaire/site/html.typ`. Typst's `line` is a paged-layout element.],
  [`<div class="x">`], [`#h("div", class: "x")[..]`], [Raw HTML has no meaning in a Typst page.],
  [`<!-- comment -->`], [`// comment`], [`/* .. */` spans lines.],
  [`---` frontmatter], [`#let frontmatter = (..)`], [The same keys; a Typst dict rather than a fenced block.],
)

Typst also has a term list, which Markdown never did:

```typ
/ Cold build: every page compiled.
/ Warm build: only what changed.
```

== Characters with special meaning

`#`, `@`, `$`, `*`, `_`, `<` and the backtick are markup. In prose, escape the
one you meant literally:

```typ
Costs \$4, tagged \#rust, mail \@example.com.
```

`#` opens code, so `#1 in the charts` is a syntax error, not a sentence. A run of literal text is easier as `#raw("...")` or a
raw span than as a line of backslashes.

== A mechanical first pass

Pandoc writes Typst, which gets the prose, lists, tables, links and footnotes
across:

```sh
pandoc -f gfm+yaml_metadata_block -t typst --wrap=preserve post.md -o post.typ
```

Without `-s` pandoc drops the metadata block, leaving a body with no
frontmatter. The simplest fix is to paste the keys back as a Typst dict. To have
pandoc write the binding instead, give it a template:

```text
// page.typ.template
#let frontmatter = (
  title: "$title$",
  date: "$date$",
)

$body$
```

```sh
pandoc -f gfm+yaml_metadata_block -t typst -s \
  --template page.typ.template post.md -o post.typ
```

The date needs no conversion: `date: "2026-07-09"` is read as an ISO date on
both Typst and Markdown pages, so the string from the Markdown file works as is.

Over a tree:

```sh
find content -name '*.md' | while read -r f; do
  pandoc -f gfm+yaml_metadata_block -t typst --wrap=preserve "$f" -o "${f%.md}.typ"
done
```

A `+++` block is not a metadata block to pandoc and comes through as escaped
text, even though baudelaire reads it as TOML. Cut it before converting:

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

Where pandoc mangles a self-contained chunk, paste the original Markdown into
`md` instead.

````typ
#import "/.baudelaire/site/markdown.typ": md

#md(```md
| Option | Default |
| ------ | ------- |
| `port` | `1821`  |
```)
````

`md` silently drops or mangles three things:

#table(
  columns: 2,
  align: (left, left),
  table.header([In the Markdown], [What survives]),
  [a raw HTML block], [*nothing*. `<ul>`, `<div>`, an embedded widget: gone from the output.],
  [`{% post_url .. %}`, `{{< relref .. >}}`, `@/blog/x.md`], [escaped text, and no longer a link.],
  [`<!--more-->`], [nothing, along with the summary it used to mark.],
)

Check for the first one. A page that was mostly hand-written HTML converts to an
empty-looking page, and the build passes because the result is valid Typst. Grep
the sources for `<` before converting, and rewrite those parts with `h()`.

== What pandoc cannot convert

*Shortcodes.* They become Typst functions. Put them in one file and import it:

```typ
// templates/parts.typ
#import "/.baudelaire/site/html.typ": h

#let aside(title, body) = h("aside", class: "note")[
  #h("strong", title)
  #body
]
```

```typ
#import "/templates/parts.typ": aside

#aside("Careful")[This is the shortcode you used to register.]
```

A shortcode that only wrapped markup often needs no function: write the markup.
A page that stayed Markdown can call the same function from a `typ eval` fence,
so a shortcode alone is no reason to convert.

*Internal links.* Point at the source file, not the URL:

```typ
See #link("../start/quickstart.typ")[the quickstart].
```

The build rewrites it to that page's permalink and fails if no page exists
there, which is how a migration finds the links it broke. Markdown pages get the
same check: `[the quickstart](../start/quickstart.md)` is resolved the same way,
and either extension may name either kind of page. An absolute URL
(`/blog/hello/`) is never checked, passes through untouched and will not follow a
later rename. Convert those.

*Raw HTML.* Rewrite anything a Markdown file embedded verbatim with `h()`. It
goes through the same typed DOM as everything else, so the asset rewriter and the
linter see it.

*Images.* `#image("cover.png")` next to a page bundle
(`content/blog/hello/index.typ`) is the direct translation of a Markdown post
with its images beside it. It publishes under the directory it sits in
(`/assets/blog/hello/cover.png`), so a tree where every post directory holds its
own `cover.png` does not collide. See #link("../../build/images.typ")[images]
for what the pipeline then does with it.

== Check the result

```sh
baudelaire check
```

This compiles every page and reports broken internal links without writing
anything. Fix what it names and run it again.
