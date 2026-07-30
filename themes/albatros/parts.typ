// The pieces both layouts are built from. Kept at the theme root rather than
// under `templates/`, so a project file can never shadow it by accident: only
// `templates/`, `assets/` and `static/` are layered.

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

// A UI label, from the site's own string table when it has one. Every visible
// word goes through here, so a French site translates the theme by configuring
// `languages { fr { strings { .. } } }` rather than by editing it.
#let label(page, key, fallback) = page.strings.at(key, default: fallback)

#let theme-toggle = h(
  "button",
  class: "toggle",
  type: "button",
  aria-label: "Toggle dark mode",
  data-theme-toggle: true,
  {
    h("span", class: "on-light", moon)
    h("span", class: "on-dark", sun)
  },
)

// `posts` -> `Posts`, for a nav built out of directory names.
#let titlecase(s) = if s == "" { s } else { upper(s.slice(0, count: 1)) + s.slice(1) }

// The top nav, derived from the build's own view of `content/` rather than from
// a menu in config: a new top-level directory shows up on its own, and one that
// goes away cannot leave a dead link behind.
#let top-nav(page) = {
  let entries = sections(page.lang).filter(s => s.pages.len() > 0 or s.children.len() > 0)
  if entries.len() > 0 {
    h("nav", class: "top-nav", aria-label: "Primary", for s in entries {
      h("a", href: "/" + s.id + "/", titlecase(s.id))
    })
  }
}

#let site-header(page) = h("header", class: "site-header", {
  h("a", class: "skip", href: "#main", label(page, "skip", "Skip to content"))
  h("a", class: "brand", href: "/", site-title)
  top-nav(page)
  theme-toggle
})

#let site-footer(page) = h("footer", class: "site-footer", {
  if author not in (none, "") { h("span", author) }
  h("span", class: "feeds", {
    h("a", href: "/rss.xml", "RSS")
    h("a", href: "/atom.xml", "Atom")
  })
})

// A date, in both forms baudelaire hands over: the machine one for `datetime`,
// the localized one for the reader. Typst's own `display` knows English month
// names only, which is why the second is not derived here.
#let posted(date) = if date != none {
  h("time", class: "date", datetime: date.iso, date.display)
}

#let reading-badge(page) = {
  let minutes = page.reading.minutes
  if minutes > 0 {
    h("span", class: "reading", str(minutes) + " " + label(page, "reading", "min read"))
  }
}

#let chips(page, terms) = if terms.len() > 0 {
  h("nav", class: "chips", aria-label: label(page, "tags", "Tags"), for term in terms {
    h("a", class: "chip", href: "/tags/" + term + "/", "#" + term)
  })
}

// Prev/next across the collection. On a reverse-dated blog `prev` is the newer
// post, which is why the labels are neutral.
#let pager(page) = {
  let nav = page.nav
  if nav.prev != none or nav.next != none {
    h("nav", class: "pager", aria-label: label(page, "pagination", "Post navigation"), {
      if nav.prev != none {
        h("a", class: "prev", rel: "prev", href: nav.prev.url, "← " + nav.prev.title)
      } else {
        h("span")
      }
      if nav.next != none {
        h("a", class: "next", rel: "next", href: nav.next.url, nav.next.title + " →")
      }
    })
  }
}

// The document shell. typst-html owns `<html>`, `<head>` and `<body>`, so this
// emits neither; the stylesheet link sits at the top of the body, which
// browsers accept and baudelaire lifts back into the head for a single-file
// export.
#let shell(page, main) = {
  let title = page.frontmatter.at("title", default: site-title)
  set document(title: title)

  h("link", rel: "stylesheet", href: "/assets/style.css")
  h("link", rel: "alternate", type: "application/rss+xml", title: site-title, href: "/rss.xml")

  site-header(page)
  h("main", class: "content", id: "main", main)
  site-footer(page)
  h("script", type: "module", src: "/assets/theme.js")
}
