#import "/templates/theme.typ": shell

// Standard content page: title + compiled body inside the site shell, with a
// tag row when the frontmatter declares tags.
#let page(page, body) = shell(
  page.frontmatter.title,
  body,
  tags: page.frontmatter.at("tags", default: ()),
)
