#import "/templates/theme.typ": pager, shell

// One documentation page. The template is bound by the collection (`template=`
// in `config.kdl`) or by a `template:` key in the page's own frontmatter, and
// receives the page and its compiled body.
#let page(page, body) = shell(
  page.frontmatter.title,
  {
    body
    pager(page.nav)
  },
  sections: page.sections,
)
