#let frontmatter = (
  title: "Configuration",
  order: 4,
)
#import "/templates/theme.typ": callout

Configuration lives in one `config.kdl` file at the project root, written in
#link("https://kdl.dev")[KDL]. Every setting has a default, so you only write
what you want to change.

== Example Configuration

```kdl
site "My Site"
url "https://example.com"
author "Ada"
lang "en"

paths {
  content "content"
  dist "public"
  assets "assets"
  static "static"
  templates "templates"
}

content {
  index "index"
  draft { build #false }
  collections {
    posts { sort "date"; reverse #true; template "post.typ"; paginate { size 10 } }
  }
  taxonomies {
    tags listing=#true
  }
}

assets {
  minify #true
  bundle #true
  fingerprint #true
  images { lazy #true; extract #true; responsive { widths 480 960 1440 }; optimize { png; jpeg quality=82 } }
}

html  { pretty #true; meta #true; anchors #true }
links { style "clean"; strict #true }

generate {
  sitemap #true
  search { formats "json"; fields "title" "body" "tags" }
  feed   { formats "rss" "atom"; limit 20 }
  cards  { template "card.typ"; width 1200; height 630 }
}

navigation {
  spa         { root "main"; prefetch "hover" }
  standalone  { file "site.html"; entry "/"; router "hash" }
  speculation { prefetch "moderate" }
}

prune #true
```

Every field has a sensible default, so a minimal `config.kdl` is just
`site "My Site"`. The rest overrides only what you name.

== How the tree is shaped

The top level is grouped by concern, and each block answers one question. Read
top to bottom and you walk the build in order:

/ #raw("paths"): where the inputs and the output directory are.
/ #raw("content"): what counts as a page, and how pages group.
/ #raw("assets"): how CSS, JavaScript, and images are processed.
/ #raw("html"), #raw("links"): what the emitted markup and its URLs look like.
/ #raw("generate"): what extra files are written beside the pages.
/ #raw("navigation"): how the browser moves between pages.

Then the machinery that runs it all (`prune`, `cache`, `hooks`, `serve`), the
ways to ship it (`deploy`, `announce`), and the knobs that cut across everything
(`typst`, `client`, `profiles`).

Two consequences of the grouping are worth knowing before you write anything:

- *Block presence is the switch.* Writing `generate { robots { } }` or
  `navigation { spa { } }` turns that feature on, and there is no `enabled` key
  to go with it. Deleting the block, or slashdashing it with `/-`, turns it back
  off.
- *The enclosing block is what gives a name its meaning.* `assets` under `paths`
  is a source directory; `assets` at the top level is the pipeline that
  processes it. Same word, different question, and they are never in the same
  scope.

== The blocks

=== Identity

/ #raw("site"), #raw("url"), #raw("author"), #raw("lang"): the site's name,
  canonical base URL, default author, and default language. `url` is what
  absolute links in feeds, the sitemap, and canonical tags are built from.
/ #raw("theme"): A theme package supplying templates, assets, and config
  defaults. See #link("../features/build/themes.typ")[themes].

=== Inputs and pages

/ #raw("paths"): Directory names, and nothing else: `content`, `dist`, `assets`,
  `templates`, and the `static` passthrough. `paths { assets }` is where your
  stylesheets and scripts are read from; what happens to them is the top-level
  `assets` block. Files under `static/` are copied verbatim to the output root
  (no processing, no fingerprint) for a `robots.txt` override, `.well-known/`, a
  `CNAME`, or an `install.sh`.
/ #raw("content"): What the content directory means. `index` names the *stem*,
  with no extension, of the file that serves a directory's own page
  (`index` makes `posts/hello/index.typ` become `/posts/hello/`, so a page can
  sit beside its images), `future` and `draft { }` decide whether
  future-dated and draft pages are built (both off by default; the `--future` /
  `--drafts` flags override per build), `collections { }` sets per-group
  sorting, permalinks, pagination, and the default template
  (see #link("../features/content/pagination.typ")[pagination]), and
  `taxonomies { }` groups pages by a frontmatter list such as `tags`
  (see #link("../features/content/taxonomies.typ")[taxonomies]).
/ #raw("languages"): Declare the languages of a multi-language site. See
  #link("../features/content/i18n.typ")[internationalization].

=== Output shape

/ #raw("assets"): The #link("../features/assets/assets.typ")[asset pipeline]:
  `minify` CSS, `bundle` JavaScript, `fingerprint` filenames with a content
  hash, and an `images { }` block for lazy loading, externalizing embedded
  images, responsive `srcset` variants, and per-format optimization. This is the
  pipeline; the directory it reads is `paths { assets }`. See
  #link("../features/discovery/meta.typ")[meta and images].
/ #raw("html"): The emitted markup: `pretty` formatting, `embed` to inline
  assets as `data:` URIs, `meta` for SEO and social tags, and `anchors` to give
  every heading a slug `id` for deep links.
/ #raw("links"): Permalink shape and link checking, one subject in one block.
  `style "clean"` gives directory-per-page permalinks, `style "flat"` gives
  `.html` files; `strict` makes a broken internal `.typ` link an error (the
  default; the `--strict-links` flag overrides), and `external` has
  #link("cli.typ")[`check`] verify outbound links over the network.

=== Extra files and navigation

/ #raw("generate"): Everything written beside the pages themselves:
  `sitemap`, #link("../features/discovery/feeds.typ")[`feed`],
  #link("../features/discovery/search.typ")[`search`],
  #link("../features/discovery/cards.typ")[`cards`] (a social image per page),
  `robots` for `robots.txt`, `llms` for `llms.txt`, and the two
  #link("deploy/static-hosts.typ")[rule files] a Pages host reads: `redirects`
  trades the #link("../features/content/redirects.typ")[redirect stubs] for a
  `_redirects` file answered with a real 301, and `headers` writes the `caching`
  policy into `_headers`.
/ #raw("navigation"): How the browser moves between pages. `spa` adds a
  client-side navigation runtime beside the ordinary pages, `standalone` folds
  the whole site into one file, and `speculation` asks the browser to prefetch
  with no runtime at all. Each is independent and off until its block exists.
  See #link("../features/build/navigating.typ")[SPA and single-file export].

=== Running the build

/ #raw("prune"): Sweep orphaned outputs (a deleted page, a renamed permalink, a
  dropped taxonomy term) out of the output directory on every build. On by
  default.
/ #raw("cache"): The incremental build cache. See
  #link("../features/build/incremental.typ")[incremental builds]. Distinct from
  `caching`, below, which is what a *browser* is told.
/ #raw("caching"): The `Cache-Control` the built files are served with, stated
  once for every destination that can say so: a `deploy` upload sets it per
  object, and `generate { headers }` writes it into `_headers`. See
  #link("deploy/s3.typ")[cache headers].
/ #raw("hooks"): Run external tools around the build. See
  #link("../features/assets/hooks.typ")[build hooks].
/ #raw("serve"): Dev-server options: `port`, `bind` address, `open` (launch a
  browser), `watch` (rebuild on change, on by default), and the `exclude` /
  `include` watch globs. See #link("../features/assets/hooks.typ")[build hooks].

=== Shipping and cross-cutting

/ #raw("deploy"): Upload target for `baudelaire deploy`. See
  #link("deploy/overview.typ")[deploying].
/ #raw("announce"): atproto/standard.site destination for `baudelaire announce`.
  See #link("../features/build/announcing.typ")[announcing].
/ #raw("typst"): Typst compiler settings: `inputs` exposes values to pages as
  `sys.inputs`, and `features` toggles experimental Typst features, `+name` on
  and `-name` off (e.g. `features "+a11y-extras"`). HTML export is always on, so
  you never list it and `-html` is refused. `registry` names a mirror of Typst
  Universe (`registry "https://packages.example.net"`) for a build that cannot
  reach `packages.typst.org`; it applies to every `@preview` package the site
  pulls, imports and #link("../features/build/themes.typ")[theme] alike. See
  #link("../features/build/context.typ")[build metadata].
/ #raw("client"): Build-time constants exposed to client JavaScript through the
  #link("../features/assets/js-modules.typ")[#raw("baudelaire:config")] virtual module.
/ #raw("profiles"): Named overlays applied with `--profile`, covered below.

#callout(kind: "note")[
  A misspelled key is a hard error with a "did you mean?" suggestion: the parser
  knows every valid key, so typos never silently do nothing. Because the
  suggestion is scoped to the block you are in, a key written at the wrong level
  is caught too.
]

== Subdirectory hosting

Give `url` a path and the whole site is served from it:

```kdl
url "https://user.codeberg.page/project"
```

Every on-page URL, links, assets, redirects, the search client, then resolves
under `/project`, and the absolute URLs in the sitemap, feeds, and canonical
tags carry it too. Only the emitted URLs shift: the files on disk keep their
plain layout (`public/guide/index.html`, never `public/project/...`), so the
host maps the path back for you. `baudelaire serve` previews under the same
path, and `--base-url` overrides it per build.

#callout(kind: "note")[
  A root domain (`url "https://example.com"`) has no path, so nothing is
  prefixed. Data files a client reads directly, `search.json` and the
  `baudelaire:*` modules, keep canonical root-relative URLs; the bundled search
  client applies the base for you.
]

== The client block, live

This site's own `client { }` block, read straight from the
#link("../features/assets/js-modules.typ")[#raw("baudelaire:config")] module and
printed by a few lines of script, the same object your bundle would import:

#html.elem("pre", attrs: (class: "config-live", "data-config": ""))[
  #html.elem("code", "// enable JavaScript to load baudelaire:config")
]

== Profiles

A `profiles` block holds named overlays applied with `--profile`:

```kdl
profiles {
  dev {
    content { draft { build #true } }
    assets { minify #false; bundle #false; fingerprint #false }
  }
}
```

`baudelaire build --profile dev` then builds drafts and skips the asset pipeline
for faster local iteration. Common flags (`--drafts`, `--base-url`, `--out`,
`--no-cache`) can also be passed directly; see the
#link("cli.typ")[CLI reference].

== Environment variables

Any string value can pull from the environment with `${VAR}`, with an optional
`${VAR:-default}` fallback. Handy for secrets and per-deploy settings you don't
want committed:

```kdl
url "${SITE_URL:-http://localhost:1821}"

typst {
  inputs {
    environment "${DEPLOY_ENV:-development}"
  }
}
```

An unset variable with no default is a hard error, not a silent empty string, so
a missing secret fails the build instead of shipping a blank value; give it a
`${VAR:-default}` fallback when an empty result is acceptable. Expansion happens
as the config is parsed, so it works in every string: paths, hook commands,
inputs, and more.

== KDL niceties

The config is plain #link("https://kdl.dev")[KDL 2.0], and a few of its features
make editing pleasant:

/ Optional quotes: simple values need no quotes. `sort=date`, `prune #true`, and
  `template=post.typ` all work; reach for quotes only when a value has spaces or
  punctuation, like a URL.
/ Slashdash: prefix any node with `/-` to comment it out whole. `/-search { }`
  turns search off without deleting your settings, which beats commenting every
  line.
/ Raw and multi-line strings: `#".."#` and `"""` let a hook command or a summary
  hold quotes and newlines with no escaping.

```kdl
generate {
  // disable search for now, but keep the settings
  /-search {
    formats "json"
  }
}

// a raw string keeps the inner quotes intact
hooks {
  before #"tailwindcss -i "assets/app.css" -o "assets/style.css" --minify"#
}
```

#callout(kind: "tip")[
  Slashdash and block-presence toggles compose nicely: a `generate { robots }`
  or `generate { llms }` block turns the feature on just by existing, and a `/-`
  in front turns it back off, no other edit required.
]
