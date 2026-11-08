#let frontmatter = (
  title: "Wren",
  order: 0,
  summary: "A small imaginary tool, documented here so the docs theme has a manual to show.",
)
// The preview builds the theme from a directory in this repository, so its
// package entrypoint is a project path. A site that installed the published
// theme writes `#import "@preview/phares:0.1.0": ..` instead.
#import "/themes/phares/lib.typ": card, cards

Wren does not exist. This manual does, so the sidebar has a tree to draw, the
search index has prose to find, and the contents on the right have headings to
follow.

= Where to start

#cards(
  card("Install", href: "/guide/install/")[Three ways to get the binary, and what to do when none of them worked.],
  card("Writing", href: "/guide/writing/")[Files, frontmatter, and what the sidebar makes of them.],
  card("Command line", href: "/reference/cli/")[Every verb, every flag.],
  card("Configuration", href: "/reference/config/")[The keys, and what they default to.],
)

Or keep pressing *Next* at the bottom of the page. One collection covers the
whole tree, so the pager runs the length of the manual and does not stop at the
end of a directory.

= What the theme is doing

Everything in the sidebar comes from `content/`. No menu is declared anywhere.
Adding a directory adds a section, and the theme's script marks the page you are
reading, since only the browser knows the current URL.

Press #html.elem("kbd")[/] anywhere to search.
