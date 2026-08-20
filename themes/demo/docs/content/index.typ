#let frontmatter = (
  title: "Wren",
  order: 0,
  summary: "A small imaginary tool, documented here so a docs theme has a manual to be.",
)
// The preview builds the theme from a directory in this repository, so its
// package entrypoint is a project path. A site that installed the published
// theme writes `#import "@preview/phares:0.1.0": ..` instead.
#import "/themes/phares/lib.typ": card, cards

Wren does not exist. This manual does, because a documentation theme with no
documentation in it proves nothing: the sidebar has to have a tree to draw, the
search index has to have prose to find, and the contents on the right have to
have headings to follow.

= Where to start

#cards(
  card("Install", href: "/guide/install/")[Three ways to get the binary, and what to do when none of them worked.],
  card("Writing", href: "/guide/writing/")[Files, frontmatter, and what the sidebar makes of them.],
  card("Command line", href: "/reference/cli/")[Every verb, every flag.],
  card("Configuration", href: "/reference/config/")[The keys, and what they default to.],
)

Or keep pressing *Next* at the bottom of the page: one collection covers the
whole tree, so the pager runs the length of the manual rather than stopping at
the end of a directory.

= What the theme is doing

Everything in the sidebar is `content/`, as the build sees it. Nothing here
declares a menu. Adding a directory adds a section, and the page you are reading
is marked in it by the theme's own script, because a URL is something only the
browser knows.

Press #html.elem("kbd")[/] anywhere to search.
