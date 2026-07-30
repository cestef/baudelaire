#let frontmatter = (
  order: 2,
  title: "Configuration",
  summary: "Collections, generated files, and profiles in config.kdl.",
  tags: ("guide",),
)
#import "/templates/theme.typ": callout

`config.kdl` is the whole build. It is written in
#link("https://kdl.dev")[KDL], and it is grouped by subject rather than by
alphabet: where files live, what a page is, how HTML comes out, what extra
files to emit.

== Collections

A collection turns a directory of pages into an ordered set:

```kdl
content {
  collections {
    guide sort="order" template="page.typ" list="list.typ"
  }
}
```

`sort="order"` reads the `order` key from each page's frontmatter, which is what
makes this a documentation site rather than a blog: chapters have a sequence, not
a date. Swap in `sort="date" reverse=#true` and you have a blog instead.

`template=` binds every page in the collection to `templates/page.typ`, so no
page has to name its own. `list=` asks for a generated index at `/guide/`,
rendered by `templates/list.typ`. Add `paginate=10` and that index splits into
numbered pages, with the pager in `list.typ` already wired for it.

A second collection is one more line. Its directory under `content/` becomes a
second group in the sidebar automatically, because the sidebar is built from the
content tree rather than from a list you maintain.

== Generated files

Anything the build emits beside the pages is opt-in, and lives under `generate`:

```kdl
generate {
  sitemap #true
  search {
    formats "json"
    fields "title" "body" "tags"
    ui #true
  }
  llms {
    summary "Documentation for this project."
  }
}
```

`search` writes `/search.json`, one document per page, from the rendered text.
`ui #true` also writes `/search.js`: a small self-mounting command palette
that fetches the index, binds `Ctrl-K` and `/`, and opens on any element with a
`data-search-open` attribute. `templates/theme.typ` loads the script and renders
that button; neither needs a line of your own JavaScript.

`llms` writes `/llms.txt`, the site as a single Markdown outline for tools that
read documentation. Drop the block and the file is not written.

#callout(kind: "warning")[
  `sitemap` and `llms` both put absolute URLs in their output, so they need the
  `url` at the top of `config.kdl` to be your real address.
]

== Headings and anchors

```kdl
html {
  pretty #true
  meta #true
  anchors #true
}
```

`anchors` gives every heading an `id` derived from its text, so any section can
be linked directly. `meta` fills in the description, OpenGraph and canonical
tags from each page's frontmatter, which is why `summary` is worth writing.

== Profiles

A profile is an overlay on everything above, selected with `--profile`:

```kdl
profiles {
  dev {
    url "http://localhost:1821"
    assets {
      minify #false
      fingerprint #false
    }
  }
}
```

Nested blocks fill in place, so the `dev` profile above keeps every other
`assets` setting and changes only the two named. Lists are the exception: they
replace wholesale, so a profile that declares `collections` declares all of
them.

Asset fingerprinting is the setting to know while writing. It folds the asset
map into every page's fingerprint, so touching `style.css` rebuilds the whole
site. That is what you want when publishing and not what you want mid-sentence,
which is the reason `--profile dev` exists.
