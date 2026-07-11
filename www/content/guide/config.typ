#frontmatter((
  title: "Configuration",
  order: 3,
  tags: ("guide", "reference"),
))
#import "/templates/theme.typ": callout

Configuration lives in one `config.kdl` file at the project root, written in
#link("https://kdl.dev")[KDL]. Every setting has a default, so you only write
what you want to change.

== A tour

```kdl
site "My Site"
url "https://example.com"
author "Ada"
lang "en"

paths {
  content "content"
  dist "public"
  assets "assets"
  templates "templates"
}

clean #true

collections {
  posts sort="date" reverse=#true paginate=10 template="post.typ"
}

taxonomies {
  tags index=#true
}

output {
  html { pretty #true; meta #true }
  images { lazy #true; optimize { png; jpeg quality=82 } }
  assets { minify #true; bundle #true; fingerprint #true }
  search { formats "json"; fields "title" "body" "tags" }
  feed { formats "rss" "atom"; limit 20 }
}
```

The `html` block controls the emitted markup (`pretty` formatting, `embed` to
inline assets as `data:` URIs, `meta` for SEO and social tags); the `images`
block handles lazy loading and per-format optimization. See
#link("../features/meta.typ")[meta and images].

Every field has a sensible default, so a minimal `config.kdl` is just
`site "My Site"`. The rest overrides only what you name.

== The blocks

/ #raw("paths"): Where content, output, assets, and templates live.
/ #raw("collections"): Per-group sorting, permalinks, pagination, and the
  default template. See #link("../features/pagination.typ")[pagination].
/ #raw("taxonomies"): Group pages by a frontmatter list such as `tags`. See
  #link("../features/taxonomies.typ")[taxonomies].
/ #raw("output"): Everything the build emits: HTML options, the
  #link("../features/assets.typ")[asset pipeline], #link("../features/search.typ")[search],
  #link("../features/feeds.typ")[feeds and sitemap], `robots.txt`, and `llms.txt`.
/ #raw("hooks"): Run external tools around the build. See
  #link("../features/hooks.typ")[build hooks].

#callout(kind: "note")[
  A misspelled key is a hard error with a "did you mean?" suggestion: the parser
  knows every valid key, so typos never silently do nothing.
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
url "${SITE_URL:-http://localhost:3000}"

typst {
  inputs {
    environment "${DEPLOY_ENV:-development}"
  }
}
```

An unset variable with no default expands to an empty string. Expansion happens
as the config is parsed, so it works in every string: paths, hook commands,
inputs, and more.

== KDL niceties

The config is plain #link("https://kdl.dev")[KDL 2.0], and a few of its features
make editing pleasant:

/ Optional quotes: simple values need no quotes. `sort=date`, `clean #true`, and
  `template=post.typ` all work; reach for quotes only when a value has spaces or
  punctuation, like a URL.
/ Slashdash: prefix any node with `/-` to comment it out whole. `/-search { }`
  turns search off without deleting your settings, which beats commenting every
  line.
/ Raw and multi-line strings: `#"…"#` and `"""` let a hook command or a summary
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
