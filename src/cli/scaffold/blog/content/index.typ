#let frontmatter = (
  title: "Home",
  template: "layout.typ",
)

Welcome to your new Baudelaire site. Every page here is Typst, compiled to HTML
and wrapped in a template you own. Edit `content/index.typ` to change this page,
drop new posts under `content/posts/`, and style everything in
`assets/style.css`.

= What is already wired up

Posts are a collection: `config.kdl` sorts them newest-first, gives them
`/posts/{slug}/` URLs, and generates an index at
#link("/posts/")[the post archive] that splits into pages once you pass five
posts. Their `tags` build #link("/tags/")[a tag index] and a page per tag, and
an RSS and Atom feed follow the same set. `templates/list.typ` renders all of
those from one file, because they are all the same thing: a titled page of
links.

Your first post is #link("/content/posts/hello.typ")[hello world]. Linking by
its `.typ` source path rather than its URL means the link is checked at build
time and survives any later change to the page's `slug` or permalink.

= Where to go next

Run `baudelaire serve` for a live preview, `baudelaire new posts/my-post` to
scaffold a post with the frontmatter its collection expects, and
`baudelaire build --profile prod` when you are ready to publish.
