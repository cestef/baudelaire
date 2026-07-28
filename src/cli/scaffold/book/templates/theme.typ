// Shared pieces of the book: the page frame, the contents list, and the
// prev/next pager. `chapter.typ` composes them; add a second template later and
// it inherits the same shell for free.
//
// `@baudelaire/*` are virtual Typst packages, served by baudelaire while it
// compiles. Nothing is downloaded, and nothing for them exists on disk, so your
// editor will underline these imports on a file that builds fine.

#import "@baudelaire/html:0.1.0": classes, h
#import "@baudelaire/site:0.1.0": author, title as book-title

// One line of the contents list.
//
// `h(tag, attr: value, ..children)` is `html.elem` without the `attrs:` wrapper:
// named arguments are attributes, positional ones are children. An attribute
// whose value is `none` is dropped, and `classes` skips a false condition and
// returns `none` for an empty result, so an ordinary entry needs no `if` and
// never gets a stray `class=""`.
#let entry(item, number: none, current: false) = h(
  "li",
  h(
    "a",
    href: item.url,
    class: classes(("current", current)),
    aria-current: if current { "page" },
    {
      if number != none { h("span", class: "num", str(number)) }
      h("span", item.title)
    },
  ),
)

// One node of `page.sections`, and then its children. A flat book only ever has
// the one `chapters` node, but a book split into parts (`content/chapters/`
// with subdirectories) nests, so this recurses rather than assuming.
#let section-list(section, here) = {
  if section.pages.len() > 0 {
    h(
      "ol",
      class: "toc-list",
      for (i, item) in section.pages.enumerate(start: 1) {
        entry(item, number: i, current: item.title == here)
      },
    )
  }
  for child in section.children {
    section-list(child, here)
  }
}

// The front page belongs to no collection, so it appears in no section: it is
// the one entry the contents list has to state itself.
#let front = (url: "/", title: "Front matter")

// The table of contents, rendered from `page.sections`: the build's own view of
// `content/`, in the collection's `sort` order. It is not a hand-kept list, so
// it cannot drift from the chapters on disk, nor from the pager below, which
// reads the same ordering.
//
// `here` is the current page's title; a page is handed no permalink of its own,
// and chapter titles are unique in a book, so that is what marks the entry the
// reader is on.
#let contents(sections, here) = h("nav", class: "toc", aria-label: "Contents")[
  #h("p", class: "toc-title", "Contents")
  #h("ol", class: "toc-list toc-front", entry(front, current: here == front.title))
  #for section in sections {
    section-list(section, here)
  }
]

// Prev / next, straight from `page.nav`. Each side is `none` at its end of the
// book, and never crosses into another collection.
#let pager(nav) = if nav.prev != none or nav.next != none {
  h("nav", class: "pager", aria-label: "Chapter")[
    #if nav.prev != none {
      h("a", class: "pager-prev", href: nav.prev.url)[
        #h("span", class: "pager-label", "Previous")
        #h("span", class: "pager-title", nav.prev.title)
      ]
    }
    #if nav.next != none {
      h("a", class: "pager-next", href: nav.next.url)[
        #h("span", class: "pager-label", "Next")
        #h("span", class: "pager-title", nav.next.title)
      ]
    }
  ]
}

// The frame every page shares. typst-html supplies `<html>`, `<head>` and
// `<body>` itself, so a template must never emit them; `set document(title: ..)`
// is how the title reaches the head it builds.
#let shell(title, body, sections: (), here: none) = {
  set document(title: title)
  h("link", rel: "stylesheet", href: "/assets/style.css")
  h("header", class: "site-header")[
    #h("a", class: "skip-link", href: "#main", "Skip to content")
    #h("a", class: "brand", href: "/", book-title)
  ]
  h("div", class: "layout")[
    #contents(sections, here)
    #h("main", class: "content", id: "main")[
      #h("article")[
        #h("h1", title)
        #body
      ]
    ]
  ]
  h("footer", class: "site-footer", author)
}
