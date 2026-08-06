#import "@baudelaire/html:0.1.0": h
#import "/templates/theme.typ": shell
#import "@baudelaire/sections:0.1.0": sections

// The changelog has a shape the generic page template cannot see: a run of
// releases, each a stack of sections, each a list of entries. The markdown it
// is written in carries that shape as heading levels, so the show rules below
// name it in the markup instead of leaving CSS and JavaScript to infer it from
// `h2` and `h3` and hope.
//
// The prose stays markdown -- it is full of tables, links and fenced samples,
// which is exactly what markdown is for. Only the frame is ours.
#let changelog(page, body) = shell(
  page.frontmatter.title,
  {
    // `CHANGELOG.md` opens with its own `# Changelog`, which the shell has
    // already drawn from the title. Dropped here rather than stripped out of
    // the file: the file is read as it stands now, and it is a repository
    // document before it is a page of this site.
    show heading.where(level: 1): none
    // A release. `## [0.0.11] - 2026-08-05` in the source; the version and the
    // date are split apart by `_changelog.ts`, which needs one element to hang
    // the rail entry off either way.
    show heading.where(level: 2): it => h("div", class: "release", data-release: true, it)
    // What kind of change follows: Upgrading, Added, Fixed. Styled per kind,
    // so the one that means "this needs your attention" reads that way.
    show heading.where(level: 3): it => h("div", class: "release-kind", it)
    body
  },
  sections: sections(page.lang),
  url: page.url,
  class: "changelog",
)
