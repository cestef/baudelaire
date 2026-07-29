#let frontmatter = (
  title: "Introduction",
  slug: "introduction",
  order: 1,
)

A book is a site with a spine. Chapters are read in sequence, the reader wants to
know where they are and what comes next, and the whole thing should be one thing
rather than a scattering of pages. This starter is set up for exactly that, and
nothing else: one collection, no tags, no feed, no archive.

= Order is data, not file names

The file this chapter lives in is `01-introduction.typ`, but the number in the
name is only there so the directory listing reads in order. What the build sorts
on is the `order` in the frontmatter above, because `config.kdl` says so:

```kdl
content {
  collections {
    chapters sort="order" template="chapter.typ"
  }
}
```

The `slug` is stated too, which is why this chapter is served at
`/chapters/introduction/` and not `/chapters/01-introduction/`. Rewriting the
running order is then a matter of changing numbers in frontmatter, and no URL
moves.

That one ordering is the only ordering. The contents list on the left is
`sections(page.lang)`, the build's own view of `content/`; the links at the foot of
this page are the `prev` and `next` of the same page's `page.nav`. Neither is a
list anyone keeps by hand, so neither can disagree with the other or with the
files on disk.

= Sections are addressable

Every heading gets an `id` derived from its own text, which is what
`html { anchors }` in the config does. The heading above answers to
#raw("#order-is-data-not-file-names"), so a section of a chapter can be linked,
cited and bookmarked, not just the chapter. The contents list could be extended
to nest a chapter's own headings under it on the same basis.

Inside a chapter, `=` is a section and `==` a subsection. The chapter's own title
is not a heading in the file at all: it comes from the frontmatter, and the
template renders it as the page's `h1`.

= One file, the whole book

The build writes `public/book.html` alongside the ordinary pages: every chapter,
the stylesheet, and a router, in a single self-contained document.

This matters more for a book than for any other kind of site. A manual gets
attached to a ticket. A thesis gets handed to an examiner. A handbook gets put on
a laptop that will spend the next eight hours somewhere without a network. In
each case the thing you want to give someone is a file, and the thing you do not
want to give them is a folder of HTML with a `public/assets/` beside it that has
to survive the trip.

Opening the file shows the front page and puts `#/` in the address bar; following
a chapter link swaps that chapter in and writes `#/chapters/first-steps/`. The
back button works. Nothing is fetched.

#link("/content/chapters/02-first-steps.typ")[The next chapter] covers adding
your own, and why the source of a book like this is Typst.
