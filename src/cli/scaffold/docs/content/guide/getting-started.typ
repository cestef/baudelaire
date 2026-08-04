#let frontmatter = (
  order: 1,
  title: "Getting started",
  description: "Run the site, add a page, and see it in the sidebar.",
  tags: ("guide",),
)
#import "/templates/theme.typ": callout

Everything in this site is a file. There is no database, no admin panel, and
nothing to register: you write a Typst file, and the build finds it.

== Run it

```sh
baudelaire serve --profile dev
```

That watches `content/`, `templates/` and `assets/`, rebuilds only what an edit
actually touched, and reloads the open browser tab. The `dev` profile (defined
at the bottom of `config.kdl`) turns off minification and asset hashing, which
keeps rebuilds to the page you are editing.

When you are ready to publish:

```sh
baudelaire build
```

The finished site lands in `public/`, ready to upload anywhere that serves
static files.

== Add a page

Create `content/guide/writing.typ`:

```typ
#let frontmatter = (
  order: 3,
  title: "Writing",
  description: "How pages are written.",
)

Pages are Typst, so you get headings, lists, code blocks, math, and your own
functions.
```

Reload. The page is at `/guide/writing/`, it is in the sidebar, and the pages
either side of it have gained a link to it in their prev/next pager. All three
come from the same place: the `guide` collection in `config.kdl` sorts by the
`order` key, and the sidebar and the pager both read that one ordering.

#callout(kind: "note")[
  A single-element array needs its trailing comma: `tags: ("guide",)`. Without
  it, Typst reads a plain string in parentheses and your tag list becomes one
  long tag.
]

== What frontmatter does

The `frontmatter` dictionary at the top of a page is read by the build before
anything is rendered. `title` names the page everywhere it is listed, `order`
places it, and `summary` is picked up by the generated `/guide/` index and by
the description meta tag. Anything else you put there is yours: it arrives in
the template as `page.frontmatter`, and in a listing as `entry.extra`.

Two keys are worth knowing early:

/ `slug`: the last URL segment, when you want it to differ from the filename.
/ `draft`: `#true` keeps the page out of a production build. `--profile dev`
  builds it anyway, so you can see your work in progress.

== Linking between pages

Link to a page's source file, not to its URL:

```typ
#link("/content/guide/configuration.typ")[Configuration]
```

Baudelaire rewrites that to the real permalink, and if the file is missing the
build fails with the offending line. Write the URL by hand and a later rename
turns it into a quiet 404. External links are checked too, with
`baudelaire check`.

== Where to go next

#link("/content/guide/configuration.typ")[Configuration] covers collections,
generated files, and profiles, which together decide the shape of the site.
