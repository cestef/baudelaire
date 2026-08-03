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
- the first build of a site, with no previous graph to guess from, compiles the
  pages that have inbound links twice.

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

== Without the switch

`page.backlinks` is always bound, so a template reading it never fails. With
`links { backlinks }` off it is simply empty, no page is compiled twice, and the
feature costs nothing.
