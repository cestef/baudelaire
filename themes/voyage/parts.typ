// The pieces both layouts are built from. Kept at the theme root rather than
// under `templates/`, so a project file cannot shadow it by accident: only
// `templates/`, `assets/` and `static/` are layered.
//
// This theme is multilingual first: every visible word is a string-table
// lookup, and the language switcher is built from the page's own editions, so a
// site that declares one language pays for none of it.

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

// A UI label. Falls back to the English word only when the language declares no
// string for it, so translating this theme is a `strings { }` block and never an
// edit to a template.
#let label(page, key, fallback) = page.strings.at(key, default: fallback)

// A language's display name from `languages` in config (`Français`). That list
// is `((code, name), ..)` and is empty on a single-language site, so a lookup
// that finds nothing falls back to the code itself.
#let lang-name(code) = {
  let declared = languages.filter(entry => entry.code == code)
  if declared.len() > 0 { declared.first().name } else { none }
}

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

// The language switcher, built from this page's own editions. `translations`
// includes the page's own, so the active one is marked rather than dropped, and
// a reader never lands on a language switch that changes the subject. Empty on
// a single-language site, where the whole element disappears.
#let lang-switch(page) = {
  if page.translations.len() > 1 {
    h("nav", class: "langs", aria-label: label(page, "languages", "Languages"), {
      for edition in page.translations {
        let active = edition.lang == page.lang
        h(
          "a",
          class: classes("lang", ("active", active)),
          href: edition.url,
          hreflang: edition.lang,
          lang: edition.lang,
          aria-current: if active { "true" },
          {
            let name = lang-name(edition.lang)
            if name != none { name } else { upper(edition.lang) }
          },
        )
      }
    })
  }
}

// `posts` -> `Posts`, for a nav built out of directory names.
#let titlecase(s) = if s == "" { s } else { upper(s.slice(0, count: 1)) + s.slice(1) }

// The nav, derived from the build's own view of `content/` in the page's own
// language: `sections(lang)` never crosses languages, so a French page's nav
// links French pages.
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
  h("div", class: "masthead", {
    h("a", class: "brand", href: "/", site-title)
    top-nav(page)
  })
  h("div", class: "controls", {
    lang-switch(page)
    theme-toggle
  })
})

#let site-footer(page) = h("footer", class: "site-footer", {
  if author not in (none, "") { h("span", author) }
  h("span", class: "feeds", {
    h("a", href: "/rss.xml", "RSS")
    h("a", href: "/atom.xml", "Atom")
  })
})

// A date in both forms baudelaire hands over: the machine one for `datetime`,
// the localized one for the reader. This is the whole reason a theme does not
// format dates itself: typst's `display` knows English month names only, so a
// hand-formatted date would read as English on every French page.
#let posted(date) = if date != none {
  h("time", class: "date", datetime: date.iso, date.display)
}

#let byline(page) = {
  let date = posted(page.date)
  let minutes = page.reading.minutes
  if date != none or minutes > 0 {
    h("p", class: "byline", {
      date
      if date != none and minutes > 0 { h("span", class: "sep", "·") }
      if minutes > 0 {
        h("span", class: "reading", str(minutes) + " " + label(page, "reading", "min read"))
      }
    })
  }
}

#let chips(page, terms) = if terms.len() > 0 {
  h("nav", class: "chips", aria-label: label(page, "tags", "Tags"), for term in terms {
    h("a", class: "chip", href: "/tags/" + term + "/", term)
  })
}

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
// emits none of them. The `hreflang` alternates a multilingual site needs are
// baudelaire's own work in the head, so the switcher above is a visible
// convenience rather than a duplicate of them.
#let shell(page, main) = {
  let title = page.frontmatter.at("title", default: site-title)
  set document(title: title)

  // Typst bakes highlight colours inline, which a runtime theme toggle cannot
  // reach. `palette.tmTheme` paints sentinel hexes instead, and the
  // `html { highlight }` block in `theme.kdl` turns each one into an `sx-*`
  // class, so the palette lives in `style.css`. The path is relative because a
  // theme resolves `/` against the project when it is a directory and against
  // the package when it is installed.
  show raw: set raw(theme: "highlight/palette.tmTheme")

  h("link", rel: "stylesheet", href: "/assets/style.css")
  h("link", rel: "alternate", type: "application/rss+xml", title: site-title, href: "/rss.xml")

  site-header(page)
  h("main", class: "content", id: "main", main)
  site-footer(page)
  h("script", type: "module", src: "/assets/theme.js")
}
