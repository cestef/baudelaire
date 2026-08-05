#import "/templates/theme.typ": shell

// The landing page draws its own hero and runs the full column width, so it asks
// `shell` for the chrome only: no sidebar, no generated `h1`.
#let home(page, body) = shell(
  page.frontmatter.title,
  body,
  sections: none,
  heading: false,
  class: "home",
  url: page.url,
)
