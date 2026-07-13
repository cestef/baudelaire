#let frontmatter = (
  title: "Search and a real asset pipeline",
  date: datetime(year: 2026, month: 7, day: 8),
  tags: ("blog", "release"),
  summary: "Client-side search with no service, plus CSS minification and JavaScript bundling through rolldown, all in a single binary.",
)
#import "/templates/theme.typ": callout

Two features landed that make Baudelaire feel complete for a documentation site:
client-side search and an asset pipeline.

== Search, without a service

Turn on #link("../features/search.typ")[search] and the build writes a static
JSON index. No account, no third-party script. The search box on this site is the
whole feature, running on that file. Prefer a prebuilt inverted index for a
smaller payload? Ask for that format instead.

== Minify, bundle, fingerprint

The #link("../features/assets.typ")[asset pipeline] runs your CSS through
Lightning CSS and your JavaScript through rolldown, the same oxc-based bundler
that powers Vite's next generation, then content-hashes the filenames and
rewrites every reference to match.

#callout(kind: "note")[
  It all stays a single binary. rolldown is a Rust crate, so there is no Node
  toolchain to install alongside the generator.
]

The cache was rebuilt around this too: rendered HTML is now content-addressed, so
identical output is stored once and unchanged pages are never rewritten. Details
in #link("../features/incremental.typ")[incremental builds].
