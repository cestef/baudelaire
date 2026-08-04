#let frontmatter = (
  title: "Front matter",
)

This is a book: an ordered run of chapters under `content/chapters/`, a contents
list down the side, and a link to the next chapter at the foot of every page.
Start reading at #link("/content/chapters/01-introduction.typ")[Introduction], or
start writing by editing this page.

= The whole book, in one file

Every build also writes `public/book.html`. That single file holds every chapter,
its own stylesheet, and a small router: open it from a downloads folder, a USB
stick, or an email attachment and the contents list, the chapter links and the
heading anchors all work, with no server involved at any point.

That is what `navigation { standalone { } }` in `config.kdl` does, and it is why
`html { embed #true }` sits above it: embedding inlines the stylesheet and any
images into the markup, so nothing in the file points at a `/assets/` path a
reader on a plane cannot fetch.

The ordinary site is still built beside it. `public/` holds every chapter as its
own page, so the book can be hosted, crawled and deep-linked exactly as before,
and handed over as one file when that is more useful.

= Where things are

/ `content/chapters/`: one file per chapter, ordered by the `order` in its
  frontmatter rather than by its file name.
/ `templates/chapter.typ`: the page: title, body, pager.
/ `templates/theme.typ`: the frame the pages share, and the contents list.
/ `assets/style.css`: all of the styling.
/ `config.kdl`: the collection, the URL shape, and the one-file export.

Link to a chapter by its `.typ` source path, as the link above does, rather than
by its URL. The build resolves it, checks it, and rewrites it, so the link
survives any later change to the chapter's `slug`, its `order`, or the
collection's permalink.
