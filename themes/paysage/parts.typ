// The pieces every layout is built from. Kept at the theme root rather than
// under `templates/`, so a project file can never shadow it by accident: only
// `templates/`, `assets/` and `static/` are layered.
//
// The work grid is the theme. It is drawn once, here, from the row shape both a
// generated listing and the `@baudelaire/pages` catalogue carry, so the landing
// page's selection and the full index at `/work/` are the same component with
// different inputs.

#import "@baudelaire/html:0.1.0": classes, h
#import "@baudelaire/pages:0.1.0": pages
#import "@baudelaire/sections:0.1.0": sections
#import "@baudelaire/site:0.1.0": author, title as site-title

// An icon, as real DOM rather than an `<img>`, so it inherits `currentColor`
// and follows the theme toggle. A theme cannot use `svg()`: those paths are
// project-root absolute, and a theme does not know where it was installed.
#let icon(..paths, size: 16) = h(
  "svg",
  class: "icon",
  width: size,
  height: size,
  viewBox: "0 0 24 24",
  fill: "none",
  stroke: "currentColor",
  stroke-width: "1.75",
  stroke-linecap: "round",
  stroke-linejoin: "round",
  aria-hidden: "true",
  ..paths.pos().map(d => h("path", d: d)),
)

#let sun = icon(
  "M12 17a5 5 0 1 0 0-10 5 5 0 0 0 0 10Z",
  "M12 1v2M12 21v2M4.2 4.2l1.4 1.4M18.4 18.4l1.4 1.4M1 12h2M21 12h2M4.2 19.8l1.4-1.4M18.4 5.6l1.4-1.4",
)

#let moon = icon("M21 12.8A9 9 0 1 1 11.2 3a7 7 0 0 0 9.8 9.8Z")

#let arrow = icon("M5 12h14", "M13 5l7 7-7 7")

// A UI label, from the site's own string table when it has one, so a
// non-English site translates the theme through config rather than by editing
// it.
#let label(page, key, fallback) = page.strings.at(key, default: fallback)

// `about` -> `About`, for a nav built out of directory names.
#let titlecase(s) = if s == "" { s } else { upper(s.slice(0, count: 1)) + s.slice(1) }

#let theme-toggle(page) = h(
  "button",
  class: "icon-btn",
  type: "button",
  aria-label: label(page, "theme", "Toggle dark mode"),
  data-theme-toggle: true,
  {
    h("span", class: "on-light", moon)
    h("span", class: "on-dark", sun)
  },
)

// The nav, derived from the build's own view of the site rather than from a
// menu in config: a new top-level directory or a new page beside the landing
// one shows up on its own, and anything removed cannot leave a dead link.
//
// Two sources, because a site has two kinds of top-level thing. `sections` are
// the content directories (`/work/`); the catalogue supplies the pages sitting
// directly under `content/` (an about page, a contact page), which belong to no
// section and so appear in no tree. The landing page is dropped from the
// second: the brand already links it.
#let top-nav(page) = {
  let directories = sections(page.lang).filter(s => s.pages.len() > 0 or s.children.len() > 0)
  let loose = pages(page.lang).filter(p => p.collection == "_root" and p.url != "/")
  h("nav", class: "top-nav", aria-label: label(page, "primary", "Primary"), {
    for s in directories { h("a", href: "/" + s.id + "/", titlecase(s.id)) }
    for entry in loose { h("a", href: entry.url, entry.label) }
  })
}

#let site-header(page) = h("header", class: "site-header", {
  h("a", class: "skip", href: "#main", label(page, "skip", "Skip to content"))
  h("a", class: "brand", href: "/", site-title)
  top-nav(page)
  theme-toggle(page)
})

#let site-footer(page) = h("footer", class: "site-footer", {
  h("div", class: "footer-line", {
    h("span", if author not in (none, "") { author } else { site-title })
    h("span", class: "feeds", h("a", href: "/rss.xml", "RSS"))
  })
})

// A date in both forms baudelaire hands over: the machine one for `datetime`,
// the localized one for the reader. Typst's own `display` knows English month
// names only, which is why the second is not derived here.
#let posted(date) = if date != none {
  h("time", class: "date", datetime: date.iso, date.display)
}

// One project, as a card. Reads the row shape a listing entry and the page
// catalogue share, so the same call renders the landing page's selection, the
// `/work/` index, and a `stack` term page.
//
// `cover` (an image path) and `role` come from the project's own frontmatter,
// which arrives whole as `entry.extra`. The summary is `entry.description`,
// which the build already resolved from `description` or its `summary` alias.
#let work-card(page, entry) = {
  let extra = entry.extra
  let cover = extra.at("cover", default: none)
  let summary = entry.description
  h("li", class: "card", h("a", class: "card-link", href: entry.url, {
    if cover != none {
      h("span", class: "card-cover", h("img", src: cover, alt: "", loading: "lazy"))
    }
    h("span", class: "card-body", {
      h("span", class: "card-head", {
        h("span", class: "card-title", entry.label)
        // The year alone, off the ISO date rather than the localized one: a
        // card has room for four characters, and "March 1, 2026" is not it.
        if entry.date != none { h("span", class: "card-year", entry.date.slice(0, count: 4)) }
      })
      if summary != none { h("span", class: "card-summary", summary) }
      let terms = entry.taxonomies.at("stack", default: ())
      if terms.len() > 0 {
        h("span", class: "card-stack", for term in terms { h("span", class: "chip", term) })
      }
    })
  }))
}

#let work-grid(page, entries) = h("ul", class: "grid", for entry in entries {
  work-card(page, entry)
})

// The lead of a landing page: the site's own words, large, plus whatever links
// the page lists in `links` (`(label, url)` pairs).
#let hero(page, body) = h("section", class: "hero", {
  h("h1", page.frontmatter.title)
  let tagline = page.frontmatter.at("tagline", default: none)
  if tagline != none { h("p", class: "tagline", tagline) }
  h("div", class: "hero-body", body)
  let links = page.frontmatter.at("links", default: ())
  if links.len() > 0 {
    h("p", class: "hero-links", for entry in links {
      h("a", class: "hero-link", href: entry.at(1), entry.at(0))
    })
  }
})

// Prev/next through the work, as one forward link: a case study ends by
// offering the next project rather than a symmetric pair of arrows.
#let next-project(page) = {
  let target = if page.nav.next != none { page.nav.next } else { page.nav.prev }
  if target != none {
    h("nav", class: "next-project", aria-label: label(page, "pagination", "Project navigation"), {
      h("a", href: target.url, {
        h("span", class: "next-label", label(page, "next-project", "Next project"))
        h("span", class: "next-title", { target.title; arrow })
      })
    })
  }
}

// The document shell. typst-html owns `<html>`, `<head>` and `<body>`, so this
// emits none of them; the stylesheet link sits at the top of the body, which
// browsers accept and baudelaire lifts back into the head for a single-file
// export.
#let shell(page, main, wide: false) = {
  let title = page.frontmatter.at("title", default: site-title)
  set document(title: title)


  h("link", rel: "stylesheet", href: "/assets/style.css")
  h("link", rel: "alternate", type: "application/rss+xml", title: site-title, href: "/rss.xml")

  site-header(page)
  h("main", class: classes("content", ("wide", wide)), id: "main", main)
  site-footer(page)
  h("script", type: "module", src: "/assets/theme.js")
}
