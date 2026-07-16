#let frontmatter = (
  title: "Forgejo & Gitea",
  order: 13,
)

The #link("github-pages.typ")[GitHub workflow] runs almost verbatim on a Forgejo
or Gitea runner: keep the `build` and `upload`/`deploy` steps, and swap the Pages
actions for whatever your instance provides (many just publish the `public/`
artifact).
