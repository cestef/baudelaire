// `@baudelaire/markdown`: markdown, rendered inside a Typst page.
//
// Generated and served by baudelaire; nothing for this exists on disk. Import
// it as `#import "@baudelaire/markdown:0.1.0": md`.

// The engine half, looked up in the global scope rather than named outright.
//
// A native function cannot be handed to a generated module, so it is bound
// globally and found here. Looked up rather than referenced so this module
// still *evaluates* under a plain `typst` compile, which is what a mirrored
// copy gets: an editor resolves `md` as a real symbol instead of choking on an
// unknown identifier, and the failure moves to the call, where the message can
// say what is actually wrong.
#let _native = dictionary(std).at(_binding, default: none)

// Markdown as content.
//
//   md("A **bold** claim.")
//
//   md(```md
//   - one
//   - two
//   ```)
//
//   md(path: "intro.md")
//
// The markdown is a string or a raw block, never a content block: `md[**a**]`
// is parsed by Typst before this function is reached, so the markdown is gone
// by then and would render as Typst markup instead.
//
// The same parser a `.md` page goes through, with this site's own
// `content { markdown { extensions } }`, so a fragment and a page agree about
// what a table or a footnote becomes.
#let md(..given) = {
  if _native == none {
    panic("`md` renders only under baudelaire, which compiles this package")
  }
  // Spread rather than unpacked, and every check left to the engine half:
  // Typst carries each argument's span through a `..` forward, so a mistake is
  // underlined where the *page* wrote it rather than somewhere inside this
  // package, which its author has never opened.
  _native(
    ..given,
    extensions: _extensions,
    html: _html,
    eval: _eval,
  )
}
