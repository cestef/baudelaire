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

collections {
  posts sort="date" reverse=#true paginate=10 template="post.typ"
}

taxonomies {
  tags index=#true
}

output {
  urls "clean"
  clean #true
  html { pretty #true; meta #true; anchors #true }
  images { lazy #true; extract #true; responsive { widths 480 960 1440 }; optimize { png; jpeg quality=82 } }
  assets { minify #true; bundle #true; fingerprint #true }
  search { formats "json"; fields "title" "body" "tags" }
  feed { formats "rss" "atom"; limit 20 }
  spa { root "main"; prefetch "hover" }
  standalone { file "site.html"; entry "/"; router "hash" }
}
```

The `html` block controls the emitted markup (`pretty` formatting, `embed` to
inline assets as `data:` URIs, `meta` for SEO and social tags, and `anchors` to
give every heading a slug `id` for deep links); the `images` block handles lazy
loading, externalizing embedded images, responsive `srcset` variants, and
per-format optimization. See
#link("../features/discovery/meta.typ")[meta and images].

`spa` and `standalone` are both off unless their block is present: the first
adds a client-side navigation runtime beside the ordinary pages, the second
folds the whole site into one file. See
#link("../features/build/navigating.typ")[SPA and single-file export].

Every field has a sensible default, so a minimal `config.kdl` is just
`site "My Site"`. The rest overrides only what you name.

== The blocks

/ #raw("paths"): Where content, output, assets, templates, and the `static`
  passthrough directory live. Files under `static/` are copied verbatim to the
  output root (no processing, no fingerprint) for a `robots.txt` override,
  `.well-known/`, a `CNAME`, or an `install.sh`.
/ #raw("client"): Build-time constants exposed to client JavaScript through the
  #link("../features/assets/js-modules.typ")[#raw("baudelaire:config")] virtual module.
/ #raw("collections"): Per-group sorting, permalinks, pagination, and the
  default template. See #link("../features/content/pagination.typ")[pagination].
/ #raw("taxonomies"): Group pages by a frontmatter list such as `tags`. See
  #link("../features/content/taxonomies.typ")[taxonomies].
/ #raw("languages"): Declare the languages of a multi-language site. See
  #link("../features/content/i18n.typ")[internationalization].
/ #raw("output"): Everything the build emits: the URL style (`urls "clean"` for
  directory-per-page permalinks, `urls "flat"` for `.html` files), whether to
  sweep orphaned files from the output directory (`clean`, on by default), HTML
  options, the #link("../features/assets/assets.typ")[asset pipeline],
  #link("../features/discovery/search.typ")[search],
  #link("../features/discovery/feeds.typ")[feeds and sitemap], `robots.txt`, and `llms.txt`.
/ #raw("hooks"): Run external tools around the build. See
  #link("../features/assets/hooks.typ")[build hooks].
/ #raw("draft"), #raw("future"): Whether to build draft or future-dated pages
  (both off by default; the `--drafts` / `--future` flags override per build).
/ #raw("links"): Broken-internal-link policy, error by default (the
  `--strict-links` flag overrides).
/ #raw("cache"): The incremental build cache. See
  #link("../features/build/incremental.typ")[incremental builds].
/ #raw("serve"): Dev-server options: `port`, `bind` address, `open` (launch a
  browser), `watch` (rebuild on change, on by default), and the `exclude` /
  `include` watch globs. See #link("../features/assets/hooks.typ")[build hooks].
/ #raw("typst"): Typst compiler settings: `inputs` exposes values to pages as
  `sys.inputs`, and `features` toggles experimental Typst features, `+name` on
  and `-name` off (e.g. `features "+a11y-extras"`). HTML export is always on, so
  you never list it and `-html` is refused. See
  #link("../features/build/context.typ")[build metadata].
/ #raw("deploy"): Upload target for `baudelaire deploy`. See
  #link("deploy/overview.typ")[deploying].
/ #raw("announce"): atproto/standard.site destination for `baudelaire announce`.
  See #link("../features/build/announcing.typ")[announcing].

#callout(kind: "note")[
  A misspelled key is a hard error with a "did you mean?" suggestion: the parser
  knows every valid key, so typos never silently do nothing.
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
    draft { build #true }
    output { assets { minify #false; bundle #false; fingerprint #false } }
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

/ Optional quotes: simple values need no quotes. `sort=date`, `future #true`, and
  `template=post.typ` all work; reach for quotes only when a value has spaces or
  punctuation, like a URL.
/ Slashdash: prefix any node with `/-` to comment it out whole. `/-search { }`
  turns search off without deleting your settings, which beats commenting every
  line.
/ Raw and multi-line strings: `#".."#` and `"""` let a hook command or a summary
  hold quotes and newlines with no escaping.

```kdl
// disable search for now, but keep the settings
/-search {
  formats "json"
}

// a raw string keeps the inner quotes intact
hooks {
  before #"tailwindcss -i "assets/app.css" -o "assets/style.css" --minify"#
}
```

#callout(kind: "tip")[
  Slashdash and block-presence toggles compose nicely: a `robots` or `llms`
  block turns the feature on just by existing, and a `/-` in front turns it back
  off, no other edit required.
]
