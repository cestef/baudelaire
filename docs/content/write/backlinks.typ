#let frontmatter = (
  title: "Backlinks",
  order: 8,
)
#import "/templates/theme.typ": callout

A backlink is a link read backwards: on the page being pointed at, the pages
that point at it. Turn it on and every page is handed `page.backlinks`.

```kdl
links {
  backlinks #true
}
```

```typ
#let post(page, body) = {
  body
  if page.backlinks.len() > 0 {
    h("nav", {
      h("h2", "Linked from")
      h("ul", for l in page.backlinks {
        h("li", h("a", href: l.url, l.title))
      })
    })
  }
}
```

Each entry is `(url, title, lang, fragments)`: where the linking page is, what
it calls itself, which edition of the site it belongs to, and which of *your*
headings it aimed at. They come ordered by URL, so the markup is the same on
every build.

== Grouping by section

`fragments` holds the heading ids a page linked to, without the `#`, and is
empty when it linked to the page rather than into it. A page that links here
three times is still one entry carrying every section it named, so a plain
"linked from" list never says one name twice.

```typ
// Everything that linked to this heading.
#let cited(page, id) = page.backlinks.filter(l => id in l.fragments)
```

```typ
#let post(page, body) = {
  body
  for id in ("install", "usage") {
    let sources = page.backlinks.filter(l => id in l.fragments)
    if sources.len() > 0 {
      h("p", "Linked to #" + id + " by " + sources.map(l => l.title).join(", "))
    }
  }
}
```

Ids are the ones baudelaire puts on your headings, the same ones a
`#link("page.typ#install")` resolves against.

== What counts as a link

Only what an author wrote in the content tree, and only
#link("pages.typ")[`.typ` links] that resolve to a page.

#table(
  columns: 2,
  align: (left, left),
  table.header([Counts], [Doesn't]),
  [`#link("../other.typ")` in a page's body], [Anything a template emits: the nav, the sidebar, `page.nav`, the backlink list itself],
  [A link in a file the page `#include`s from `content/`], [Links on a generated listing or taxonomy term page],
  [Two links to one page, as one entry], [A link to the page's own URL, section or not],
  [A link to `other.typ#section`, as a link to the page *and* to the section], [An external or non-`.typ` href],
)

Without the first rule a shared layout would make every page a backlink of every
other, and the list would say nothing. Without the second, a tag page would
backlink every page carrying the tag.

== What it costs

The pages linking to a page are known only once every page has rendered, which
is after the point a page has to be compiled. So a build compiles each page
against the backlinks the *last* build recorded, then checks that guess against
the graph it actually produced and compiles again the pages that disagree.

That means:

- an edit that changes no links repairs nothing, and every unrelated page stays
  a cache hit;
- adding or removing a link recompiles the page at the other end of it, and
  nothing else;
- retitling a page rewrites the pages it links to, since the title is in their
  backlink lists;
- the first build of a site has no previous graph to guess from, so it guesses
  from the sources instead: the `.typ` links a page writes out literally are
  read straight off it. A site whose links are written by hand (nearly every
  site) compiles each page once even cold; one that builds links in a loop pays
  a second compile for the pages that were guessed wrong.

Sidecars are not redrawn by the second compile: a #link("../build/generate/cards.typ")[social card]
and a #link("../build/generate/pdf.typ")[PDF] carry no backlinks, and redrawing
one per repaired page would cost more than the pass it repairs.

#callout(kind: "warn")[
  Content that *links somewhere different depending on its own backlinks* has no
  answer to settle on: every repair moves the graph again. The build stops after
  the second attempt, ships what it last computed, and warns
  (`baudelaire::backlinks::unstable`) naming the pages. Displaying backlinks is
  always safe; branching your own links on them is not.
]

== Orphans

The same edges, read the other way: the pages nothing links to.

```kdl
links {
  orphans "any"        // or "authored"
}
```

```
baudelaire::links::orphans

  ⚠ 2 pages linked from nowhere
  help: link each from a page that is reachable, or drop `links { orphans }`

  ⚠ `guide/exporting.typ` is linked from nowhere, and serves at `/guide/exporting/`
  ⚠ `notes/scratch.typ` is linked from nowhere, and serves at `/notes/scratch/`
```

A link counts when an author wrote it, in prose, whether spelled as a `.typ`
path or as a URL. A layout never counts: a sidebar links every page from every
page, so counting one would mean nothing is ever an orphan. What the mode
decides is whether the build's own listings count.

#table(
  columns: 2,
  align: (left, left),
  table.header([Mode], [Names a page when]),
  [`any`], [nothing points at it at all, a #link("collections/pagination.typ")[paginated index] and a #link("collections/taxonomies.typ")[term page] included. A blog post reached from `/blog/` is reached, so the report names only what a reader cannot get to.],
  [`authored`], [nobody *wrote* about it, an index being no answer. The question a documentation site asks; on a blog it names every post, which is the trade.],
)

Left out of the report itself, since nobody forgot to link them: the root of
each language, the generated listings, and the
#link("../ship/navigating.typ")[not-found page].

A listing's entries come from the page set and not from its markup, so a
listing with a #link("templates.typ")[template] of its own counts the same as
the default one: the links its template draws are the template's, like any
other chrome.

So a page it names is one a reader can only reach by knowing the URL. It is a
report and never a failure: a landing page linked from a hand-written menu and
from nowhere else is an ordinary thing to have, and only you can say which of
these is a mistake. `baudelaire build --strict` turns every warning into a
failure if you want it enforced in CI.

Either switch turns on the link graph, so a site that wants only the report pays
for the edges and none of the second compiles.

== Without the switch

`page.backlinks` is always bound, so a template reading it never fails. With
`links { backlinks }` off it is simply empty, no page is compiled twice, and the
feature costs nothing.
