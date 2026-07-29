#import "/templates/theme.typ": pager, shell
#import "@baudelaire/sections:0.1.0": sections

// The template function is the file's stem, so `chapter.typ` exports `chapter`.
// It receives the page (frontmatter, nav, sections, ..) and the compiled body.
//
// The collection binds this to every chapter; `content/index.typ` names it in
// its own frontmatter. The front page belongs to no collection, so its
// `page.nav` is empty on both sides and the pager renders nothing.
#let chapter(page, body) = shell(
  page.frontmatter.title,
  {
    body
    pager(page.nav)
  },
  sections: sections(page.lang),
  here: page.frontmatter.title,
)
