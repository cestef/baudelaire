#let frontmatter = (
  title: "Configuration",
  order: 1,
)
#import "/templates/theme.typ": callout

One `config.kdl` at the project root, in #link("https://kdl.dev")[KDL]. Every key
has a default, so you only write what you want to change.

```kdl
site "My Site"
url "https://example.com"
author "Ada"
lang "en"

paths {
  content "content"
  dist "public"
}

links {
  style "clean"
}

generate {
  sitemap #true
  feed {
    formats "rss" "atom"
    limit 20
  }
}
```

A one-line `config.kdl` holding `site "My Site"` is a valid site. `--config`
points at a file somewhere else, `--root` at a different project directory.

== KDL in one minute

```kdl
// a line comment
prune #true                 // booleans are #true and #false
serve { port 1821 }         // a block groups keys under a name
links { style clean }       // a simple word needs no quotes
hooks {
  before #"tailwindcss -i "assets/_app.css" -o "assets/style.css""#
}
```

Booleans are `#true` and `#false`. A bare word parses as a string, so
`style clean` and `style "clean"` are the same; quote anything with spaces,
slashes, or punctuation. A raw string `#"..."#` keeps inner quotes intact, which
is what a hook command usually needs, and `"""` opens a multi-line one.

== The shape of the tree

The top level is grouped by concern. Each block answers one question, and the
page that answers it in full is on the right.

#table(
  columns: 3,
  align: (left, left, left),
  table.header([Block], [Answers], [See]),

  [`site`, `url`, `lang`, `author`, `description`],
  [The site's name, canonical base URL, default language, author, and what it is in one line.],
  [#link("reference.typ")[reference]],

  [`theme`],
  [The templates and assets this site layers over.],
  [#link("../start/themes.typ")[themes]],

  [`paths`],
  [Where the content, output, asset, template and static trees live.],
  [#link("../write/pages.typ")[pages]],

  [`content`],
  [What counts as a page, and how pages group.],
  [#link("../write/pages.typ")[pages], #link("../write/collections/defining.typ")[collections]],

  [`languages`],
  [The languages of a multi-language site.],
  [#link("../write/i18n.typ")[multiple languages]],

  [`assets`],
  [How CSS, JavaScript and images are processed.],
  [#link("../build/assets.typ")[asset pipeline], #link("../build/images.typ")[images]],

  [`html`],
  [What the emitted markup carries.],
  [#link("../write/pages.typ")[pages], #link("../build/generate/meta.typ")[meta tags]],

  [`links`],
  [Permalink shape, and how hard a broken link fails.],
  [#link("../write/pages.typ")[pages]],

  [`redirect`],
  [Old paths no page owns, and where each one moved.],
  [#link("../write/collections/redirects.typ")[redirects]],

  [`lint`],
  [Checks and size budgets over the built pages.],
  [#link("../build/linting.typ")[linting & budgets]],

  [`security`],
  [Integrity hashes and the content security policy.],
  [#link("../ship/security.typ")[integrity & CSP]],

  [`generate`],
  [What gets written beside the pages.],
  [#link("../build/generate/search.typ")[search], #link("../build/generate/feeds.typ")[feeds], #link("../build/generate/cards.typ")[cards], #link("../build/generate/pdf.typ")[PDFs], #link("../ship/hosts/static-hosts.typ")[static hosts]],

  [`navigation`],
  [How a visitor moves between built pages.],
  [#link("../ship/navigating.typ")[SPA & single-file export]],

  [`prune`],
  [Whether output this build did not produce is deleted.],
  [#link("reference.typ")[reference]],

  [`cache`],
  [Where incremental build state lives.],
  [#link("../build/incremental.typ")[incremental builds]],

  [`caching`],
  [The `Cache-Control` uploaded files are given.],
  [#link("../ship/hosts/s3.typ")[S3 storage]],

  [`typst`],
  [Compiler features, `sys.inputs`, package registry.],
  [#link("../lookup/context.typ")[build metadata]],

  [`client`],
  [Constants handed to client JavaScript.],
  [#link("../lookup/js-modules.typ")[JS modules]],

  [`hooks`],
  [External commands run around the build.],
  [#link("../build/hooks.typ")[build hooks]],

  [`announce`],
  [Where the site announces its own metadata.],
  [#link("../ship/announce.typ")[announcing]],

  [`deploy`],
  [Where `baudelaire deploy` uploads the built site.],
  [#link("../ship/deploy.typ")[deploying]],

  [`serve`],
  [The dev server: port, watching, editor.],
  [#link("../build/preview.typ")[dev server & preview]],

  [`profiles`],
  [Named overlays applied with `--profile`.],
  [#link("profiles.typ")[profiles & environment]],
)

Every key, with its value shape and what it does, is in the
#link("reference.typ")[config reference]. `baudelaire reference assets.images`
prints one subtree of the same thing in the terminal.

The enclosing block is what gives a name its meaning. `paths { assets }` is the
directory your stylesheets are read from; the top-level `assets` block is the
pipeline that processes them. Same word, different question, never the same
scope.

== A block's presence is the switch

There is no `enabled` key. Writing the block turns the feature on, and a block
with nothing to configure can be written bare:

```kdl
generate {
  llms
  robots {
    disallow "/drafts/"
  }
}
```

That writes both `llms.txt` and `robots.txt`. Delete the node and neither is
written.

#callout(kind: "note")[
  Only a block that has a switch to flip accepts the bare spelling. `paths` or
  `html` on their own configure nothing, so they error rather than being read as
  intent.
]

== Commenting a block out

`/-` in front of any node comments it out whole, settings intact:

```kdl
generate {
  sitemap #true

  /-search {
    formats "json"
    fields "title" "body" "tags"
  }
}
```

Search is off, and turning it back on is deleting two characters. This composes
with presence-as-a-switch: `/-llms` is how you keep the `llms { }` block you
tuned without emitting the file.

== Hosting under a subdirectory

Give `url` a path and the whole site is served from it:

```kdl
url "https://user.github.io/project"
```

Links, assets, redirects and the search client all resolve under `/project`, and
the absolute URLs in the sitemap, feeds and canonical tags carry it. Only the
emitted URLs shift. The files on disk keep their plain layout
(`public/blog/index.html`, never `public/project/blog/index.html`), so the host
maps the path back for you. `baudelaire serve` previews under the same path, and
`--base-url` overrides it for one build.

#callout(kind: "note")[
  A root domain has no path, so nothing is prefixed. `search.json` and the
  `baudelaire:*` modules keep root-relative URLs either way; the bundled search
  client applies the base itself.
]

== Typos are errors

The parser knows every valid key, so a misspelling fails the build instead of
silently doing nothing:

```text
unknown config key `prety`
  help: did you mean `pretty`?
        valid keys: `pretty`, `embed`, `meta`, `anchors`, ...
```

The suggestion is scoped to the block you are in, so a real key written at the
wrong level is caught too.
