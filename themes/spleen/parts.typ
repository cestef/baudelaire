// The pieces both layouts are built from. Kept at the theme root rather than
// under `templates/`, so a project file cannot shadow it by accident: only
// `templates/`, `assets/` and `static/` are layered.
//
// Nothing here emits a `<script>`. The theme is a terminal, the colours come
// from the reader's own preference, and there is nothing left for script to do.

#import "@baudelaire/html:0.1.0": classes, h
#import "@baudelaire/sections:0.1.0": sections
#import "@baudelaire/site:0.1.0": author, title as site-title

// A UI label, from the site's own string table when it has one, so a
// non-English site translates the theme through config rather than by editing
// it.
#let label(page, key, fallback) = page.strings.at(key, default: fallback)

// `posts` -> `posts`, but a path segment, which is what the prompt shows.
#let path-of(page) = page.frontmatter.at("title", default: "")

// The nav, derived from the build's own view of `content/` rather than from a
// menu in config: a new top-level directory appears on its own, and one that
// goes away cannot leave a dead link behind. Rendered as commands, because that
// is the joke this theme is telling.
#let nav(page) = {
  let entries = sections(page.lang).filter(s => s.pages.len() > 0 or s.children.len() > 0)
  if entries.len() > 0 {
    h("nav", class: "nav", aria-label: "Primary", for s in entries {
      h("a", class: "cmd", href: "/" + s.id + "/", "cd " + s.id)
    })
  }
}

#let site-header(page) = h("header", class: "masthead", {
  h("a", class: "skip", href: "#main", label(page, "skip", "Skip to content"))
  h("div", class: "prompt", {
    h("span", class: "user", "visitor")
    h("span", class: "at", "@")
    h("a", class: "host", href: "/", site-title)
    h("span", class: "cwd", ":~$")
  })
  nav(page)
})

#let site-footer(page) = h("footer", class: "footer", {
  h("span", class: "rule", "─" * 3)
  h("div", class: "footer-line", {
    if author not in (none, "") { h("span", "© " + author) }
    h("span", class: "feeds", {
      h("a", href: "/rss.xml", "rss")
      h("a", href: "/atom.xml", "atom")
      h("a", href: "/sitemap.xml", "sitemap")
    })
  })
})

// A date in both forms baudelaire hands over: the machine one for `datetime`,
// the localized one for the reader. Typst's own `display` knows English month
// names only, which is why the second is not derived here.
#let posted(date) = if date != none {
  h("time", class: "date", datetime: date.iso, date.display)
}

#let meta-line(page) = {
  let date = posted(page.date)
  let minutes = page.reading.minutes
  let terms = page.taxonomies.at("tags", default: ())
  if date != none or minutes > 0 or terms.len() > 0 {
    h("p", class: "meta", {
      if date != none { date }
      if minutes > 0 {
        h("span", class: "words", str(minutes) + " " + label(page, "reading", "min read"))
      }
      if terms.len() > 0 {
        h("span", class: "tags", for term in terms {
          h("a", class: "tag", href: "/tags/" + term + "/", "[" + term + "]")
        })
      }
    })
  }
}

#let pager(page) = {
  let nav = page.nav
  if nav.prev != none or nav.next != none {
    h("nav", class: "pager", aria-label: label(page, "pagination", "Post navigation"), {
      if nav.prev != none {
        h("a", class: "prev", rel: "prev", href: nav.prev.url, "<- " + nav.prev.title)
      } else {
        h("span")
      }
      if nav.next != none {
        h("a", class: "next", rel: "next", href: nav.next.url, nav.next.title + " ->")
      }
    })
  }
}

// The document shell. typst-html owns `<html>`, `<head>` and `<body>`, so this
// emits none of them; the stylesheet link sits at the top of the body, which
// browsers accept and baudelaire lifts back into the head for a single-file
// export.
#let shell(page, main) = {
  let title = page.frontmatter.at("title", default: site-title)
  set document(title: title)

  h("link", rel: "stylesheet", href: "/assets/style.css")
  h("link", rel: "alternate", type: "application/rss+xml", title: site-title, href: "/rss.xml")

  site-header(page)
  h("main", class: "screen", id: "main", main)
  site-footer(page)
}
