// The pieces every layout is built from. Kept at the theme root rather than
// under `templates/`, so a project file can never shadow it by accident: only
// `templates/`, `assets/` and `static/` are layered.
//
// The sidebar is the whole theme, and it is not authored anywhere: it is
// `@baudelaire/sections`, the build's own view of `content/`, so a new page
// appears in the nav by existing and a deleted one cannot leave a dead link.

#import "@baudelaire/html:0.1.0": classes, h
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

#let magnifier = icon("M11 18a7 7 0 1 0 0-14 7 7 0 0 0 0 14Z", "M20 20l-4.2-4.2")

#let menu = icon("M4 7h16M4 12h16M4 17h16")

// A UI label, from the site's own string table when it has one, so a
// non-English site translates the theme through config rather than by editing
// it.
#let label(page, key, fallback) = page.strings.at(key, default: fallback)

// `getting-started` -> `Getting started`, for a nav built out of directory
// names. A directory that wants another name gets one from the site's string
// table, keyed by its id.
#let titlecase(s) = {
  let words = s.split("-")
  let first = words.first()
  let head = if first == "" { first } else { upper(first.slice(0, count: 1)) + first.slice(1) }
  (head,) + words.slice(1)
} .join(" ")

#let section-title(page, id) = label(page, id, titlecase(id))

// One nav node and everything under it. A directory with children nests as a
// `<details>`, which is open by default: a manual should show its shape at a
// glance, and a reader who collapses one keeps it collapsed (the theme's script
// remembers).
#let nav-node(page, section, depth: 0) = {
  let links = h("ul", class: "nav-list", for entry in section.pages {
    h("li", h("a", class: "nav-link", href: entry.url, entry.title))
  })
  let children = section.children.map(child => nav-node(page, child, depth: depth + 1))
  if section.children.len() > 0 {
    h("details", class: "nav-group", open: true, data-nav-group: section.id, {
      h("summary", class: "nav-title", section-title(page, section.id))
      h("div", class: "nav-body", { links; children.join() })
    })
  } else {
    h("div", class: "nav-group", {
      h("p", class: "nav-title", section-title(page, section.id))
      links
    })
  }
}

#let sidebar(page) = h(
  "nav",
  class: "sidebar",
  id: "sidebar",
  aria-label: label(page, "documentation", "Documentation"),
  {
    let tree = sections(page.lang).filter(s => s.pages.len() > 0 or s.children.len() > 0)
    for section in tree { nav-node(page, section) }
  },
)

// The on-page contents. Deliberately empty markup: the headings live in the
// compiled body, so the theme's script fills this from the rendered page rather
// than the layout guessing at a structure it cannot see. A page with fewer than
// two headings gets no contents at all, which the script decides.
#let contents(page) = h(
  "nav",
  class: "toc",
  aria-label: label(page, "contents", "On this page"),
  data-toc: true,
  hidden: true,
  {
    h("p", class: "toc-title", label(page, "contents", "On this page"))
    h("ol", class: "toc-list")
  },
)

#let search-trigger(page) = h(
  "button",
  class: "search-trigger",
  type: "button",
  aria-label: label(page, "search", "Search"),
  aria-keyshortcuts: "Control+K /",
  data-search-open: true,
  {
    magnifier
    h("span", class: "search-label", label(page, "search", "Search"))
    h("kbd", "/")
  },
)

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

#let site-header(page) = h("header", class: "site-header", {
  h("a", class: "skip", href: "#main", label(page, "skip", "Skip to content"))
  h(
    "button",
    class: "icon-btn nav-toggle",
    type: "button",
    aria-label: label(page, "navigation", "Toggle navigation"),
    aria-expanded: "false",
    aria-controls: "sidebar",
    data-nav-toggle: true,
    menu,
  )
  h("a", class: "brand", href: "/", site-title)
  h("div", class: "header-tools", {
    search-trigger(page)
    theme-toggle(page)
  })
})

#let chips(page, terms) = if terms.len() > 0 {
  h("nav", class: "chips", aria-label: label(page, "tags", "Tags"), for term in terms {
    h("a", class: "chip", href: "/tags/" + term + "/", term)
  })
}

// Prev/next through the manual. One collection covers the whole tree, so these
// run past a directory boundary instead of stopping at it.
#let pager(page) = {
  let nav = page.nav
  if nav.prev != none or nav.next != none {
    h("nav", class: "pager", aria-label: label(page, "pagination", "Page navigation"), {
      if nav.prev != none {
        h("a", class: "prev", rel: "prev", href: nav.prev.url, {
          h("span", class: "pager-label", label(page, "previous", "Previous"))
          h("span", class: "pager-title", nav.prev.title)
        })
      } else {
        h("span")
      }
      if nav.next != none {
        h("a", class: "next", rel: "next", href: nav.next.url, {
          h("span", class: "pager-label", label(page, "next", "Next"))
          h("span", class: "pager-title", nav.next.title)
        })
      }
    })
  }
}

#let page-footer(page) = h("footer", class: "site-footer", {
  h("span", if author not in (none, "") { author } else { site-title })
  h("span", class: "meta", label(page, "built", "Built with baudelaire"))
})

// The document shell: header, sidebar, article, contents. typst-html owns
// `<html>`, `<head>` and `<body>`, so this emits none of them; the stylesheet
// link sits at the top of the body, which browsers accept and baudelaire lifts
// back into the head for a single-file export.
#let shell(page, main, toc: true) = {
  let title = page.frontmatter.at("title", default: site-title)
  set document(title: title)


  h("link", rel: "stylesheet", href: "/assets/style.css")

  site-header(page)
  h("div", class: "layout", {
    sidebar(page)
    h("main", class: "content", id: "main", main)
    if toc { contents(page) }
  })
  page-footer(page)
  // The palette client `generate { search { ui } }` emits, and this theme's own
  // script. Both are modules, so neither blocks the render.
  h("script", type: "module", src: "/search.js")
  h("script", type: "module", src: "/assets/theme.js")
}
