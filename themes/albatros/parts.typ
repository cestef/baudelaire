// The pieces every layout is built from. Kept at the theme root rather than
// under `templates/`, so a project file can never shadow it by accident: only
// `templates/`, `assets/` and `static/` are layered.
//
// Every visible word goes through `label`, and every date arrives already
// localized, so translating this theme is a `strings { }` block in config and
// never an edit to a file here.

#import "@baudelaire/html:0.1.0": classes, h
#import "@baudelaire/sections:0.1.0": sections
#import "@baudelaire/site:0.1.0": author, languages, title as site-title

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

#let globe = icon(
  "M12 21a9 9 0 1 0 0-18 9 9 0 0 0 0 18Z",
  "M3.6 9h16.8M3.6 15h16.8",
  "M12 3a14 14 0 0 1 0 18a14 14 0 0 1 0-18Z",
)

// A UI label, from the site's own string table when it has one. Falls back to
// the English word only when the language declares none.
#let label(page, key, fallback) = page.strings.at(key, default: fallback)

// A language's display name from `languages` in config (`Français`). The list
// is empty on a single-language site, so a lookup that finds nothing falls back
// to the uppercased code.
#let lang-name(code) = {
  let declared = languages.filter(entry => entry.code == code)
  if declared.len() > 0 { declared.first().name } else { upper(code) }
}

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

// The language switcher, built from this page's own editions, so a reader never
// lands on a language switch that changes the subject. `translations` includes
// the page's own edition, so the active one is marked rather than dropped, and
// the whole element disappears on a single-language site.
#let lang-switch(page) = if page.translations.len() > 1 {
  h("nav", class: "langs", aria-label: label(page, "languages", "Languages"), {
    h("span", class: "langs-icon", globe)
    for edition in page.translations {
      let active = edition.lang == page.lang
      h(
        "a",
        class: classes("lang", ("active", active)),
        href: edition.url,
        hreflang: edition.lang,
        lang: edition.lang,
        aria-current: if active { "true" },
        lang-name(edition.lang),
      )
    }
  })
}

// `posts` -> `Posts`, for a nav built out of directory names.
#let titlecase(s) = if s == "" { s } else { upper(s.slice(0, count: 1)) + s.slice(1) }

// The top nav, derived from the build's own view of `content/` rather than from
// a menu in config: a new top-level directory shows up on its own, and one that
// goes away cannot leave a dead link behind. `sections(lang)` never crosses
// languages, so a French page's nav links French pages.
#let top-nav(page) = {
  let entries = sections(page.lang).filter(s => s.pages.len() > 0 or s.children.len() > 0)
  if entries.len() > 0 {
    h("nav", class: "top-nav", aria-label: label(page, "primary", "Primary"), for s in entries {
      h("a", href: "/" + s.id + "/", titlecase(s.id))
    })
  }
}

#let site-header(page) = h("header", class: "site-header", {
  h("a", class: "skip", href: "#main", label(page, "skip", "Skip to content"))
  h("a", class: "brand", href: "/", site-title)
  top-nav(page)
  h("div", class: "controls", {
    lang-switch(page)
    theme-toggle(page)
  })
})

#let site-footer(page) = h("footer", class: "site-footer", {
  h("span", if author not in (none, "") { author } else { site-title })
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

#let byline(page) = {
  let date = posted(page.date)
  let reading = reading-badge(page)
  if date != none or reading != none {
    h("p", class: "byline", {
      date
      if date != none and reading != none { h("span", class: "sep", aria-hidden: "true", "·") }
      reading
    })
  }
}

#let chips(page, terms) = if terms.len() > 0 {
  h("nav", class: "chips", aria-label: label(page, "tags", "Tags"), for term in terms {
    h("a", class: "chip", href: "/tags/" + term + "/", "#" + term)
  })
}

// One row of a page list, from the row shape every list is made of: a generated
// listing's `page.frontmatter.entries` and the `@baudelaire/pages` catalogue
// carry the same fields, so this one function renders an index, a term page,
// and the home page's recent posts.
#let entry-row(page, entry) = h("li", class: classes("entry", ("dated", entry.date != none)), {
  h("a", class: "entry-title", href: entry.url, entry.label)
  let terms = entry.taxonomies.at("tags", default: ())
  if entry.date != none or entry.note != none or terms.len() > 0 {
    h("p", class: "entry-meta", {
      if entry.date != none {
        h("time", class: "date", datetime: entry.date, entry.display)
      }
      if entry.note != none { h("span", class: "count", entry.note) }
      if terms.len() > 0 {
        h("span", class: "entry-tags", for term in terms {
          h("a", class: "chip", href: "/tags/" + term + "/", "#" + term)
        })
      }
    })
  }
  let summary = entry.extra.at("summary", default: entry.extra.at("description", default: none))
  if summary != none { h("p", class: "entry-summary", summary) }
})

#let entry-list(page, entries) = h("ul", class: "listing", for entry in entries {
  entry-row(page, entry)
})

// Prev/next across the collection. On a reverse-dated blog `prev` is the newer
// post, which is why the labels are neutral.
#let pager(page) = {
  let nav = page.nav
  if nav.prev != none or nav.next != none {
    h("nav", class: "pager", aria-label: label(page, "pagination", "Post navigation"), {
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

// The document shell. typst-html owns `<html>`, `<head>` and `<body>`, so this
// emits none of them; the stylesheet link sits at the top of the body, which
// browsers accept and baudelaire lifts back into the head for a single-file
// export. The `hreflang` alternates a multilingual site needs are baudelaire's
// own work in the head, so the switcher above is a visible convenience rather
// than a duplicate of them.
#let shell(page, main) = {
  let title = page.frontmatter.at("title", default: site-title)
  set document(title: title)

  // Typst bakes highlight colours inline, which a runtime theme toggle cannot
  // reach. `palette.tmTheme` paints sentinel hexes instead, and the
  // `html { highlight }` block in `theme.kdl` turns each one into an `sx-*`
  // class, so the real palette lives in `style.css`. The path is relative
  // because a theme resolves `/` against the project when it is a directory and
  // against the package when it is installed.
  show raw: set raw(theme: "highlight/palette.tmTheme")

  h("link", rel: "stylesheet", href: "/assets/style.css")
  h("link", rel: "alternate", type: "application/rss+xml", title: site-title, href: "/rss.xml")

  site-header(page)
  h("main", class: "content", id: "main", main)
  site-footer(page)
  h("script", type: "module", src: "/assets/theme.js")
}
