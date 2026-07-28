#import "@baudelaire/html:0.1.0": classes, h, svg

#let link-to(href, label) = h("a", href: href, label)

// Build metadata, injected by baudelaire at `sys.inputs.baudelaire`. Only
// values that change between builds are read here (site identity comes from
// `@baudelaire/site`), so a page depends on the commit only if it shows it.
#let build = sys.inputs.at("baudelaire", default: (:))
#let git = build.at("git", default: none)

// Lucide icons, inlined from `icons/` as real DOM so they inherit
// `currentColor` and recolour with the theme toggle. They were transcribed by
// hand into a Typst dict until `svg()` existed, because an `<img>` cannot be
// styled from the page.
#let lucide(name, size: 18) = svg(
  "/icons/" + name + ".svg",
  width: size, height: size, class: "icon", aria-hidden: "true",
)

#let search-trigger = h(
  "button",
  class: "search-trigger",
  type: "button",
  aria-label: "Search",
  aria-keyshortcuts: "Control+K /",
  data-search-open: true,
  {
    lucide("search", size: 16)
    h("span", class: "search-trigger-label", "Search")
    h("kbd", class: "search-trigger-key", "/")
  },
)

#let theme-toggle = h(
  "button",
  class: "icon-btn",
  type: "button",
  aria-label: "Toggle dark mode",
  aria-pressed: "false",
  data-theme-toggle: true,
  {
    h("span", class: "icon-moon", aria-hidden: "true", lucide("moon"))
    h("span", class: "icon-sun", aria-hidden: "true", lucide("sun"))
  },
)

#let nav-toggle = h(
  "button",
  class: "icon-btn nav-toggle",
  type: "button",
  aria-label: "Toggle navigation",
  aria-expanded: "false",
  aria-controls: "sidebar",
  data-nav-toggle: true,
  lucide("menu"),
)

#let site-header = h("header", class: "site-header")[
  #h("a", class: "skip-link", href: "#main", "Skip to content")
  #h("a", class: "brand", href: "/", "Baudelaire")
  #h("nav", class: "top-nav", aria-label: "Primary")[
    #link-to("/guide/install/", "Guide")
    #link-to("/features/", "Features")
    #link-to("/blog/", "Blog")
  ]
  #search-trigger
  #theme-toggle
  #nav-toggle
]

#let nav-group(title, items) = h("div", class: "nav-group")[
  #h("p", class: "nav-title", title)
  #h("ul", for (href, label) in items { h("li", link-to(href, label)) })
]

// Title-case a directory name for display: `storage` -> `Storage`.
#let _titlecase(s) = if s == "" { s } else { upper(s.slice(0, count: 1)) + s.slice(1) }

// Render a section from `page.sections` recursively: its direct-page links,
// then each child directory as a nested subgroup. `page.sections` is a tree of
// `(id, pages: ((url, title), ..), children: (..))` mirroring `content/`.
// Top-level groups are static headings; nested subsections render as native
// `<details>` so they collapse without any script. They ship closed; JS opens
// the section holding the current page and restores any the reader expanded.
#let nav-section(title, section, sub: false) = {
  let body = {
    if section.pages.len() > 0 {
      h("ul", for p in section.pages { h("li", link-to(p.url, p.title)) })
    }
    for child in section.children {
      nav-section(_titlecase(child.id), child, sub: true)
    }
  }
  if sub {
    h("details", class: "nav-group nav-sub", data-nav-section: section.id)[
      #h("summary", class: "nav-title", {
        h("span", title)
        lucide("chevron-right", size: 14)
      })
      #h("div", class: "nav-sub-body", body)
    ]
  } else {
    h("div", class: "nav-group")[
      #h("p", class: "nav-title", title)
      #body
    ]
  }
}

// The doc collections shown in the sidebar, as `(id, display title)`. Their
// pages, and their order, come from `page.sections` (the build's own view of
// the site), so the sidebar can never drift from the content or from the
// prev/next pager, which read the same source.
#let _doc-groups = (("guide", "Guide"), ("features", "Features"))

#let sidebar(sections) = {
  // `sections` is a tree: `(id, pages: ((url, title), ..), children: (..))`,
  // one node per `content/` directory, in each collection's sort order.
  let by-id = (:)
  for section in sections {
    by-id.insert(section.id, section)
  }
  h("nav", class: "sidebar", id: "sidebar", aria-label: "Documentation", {
    for (id, title) in _doc-groups {
      let section = by-id.at(id, default: none)
      if section != none and (section.pages.len() > 0 or section.children.len() > 0) {
        nav-section(title, section)
      }
    }
    nav-group("More", (
      ("/blog/", "Blog"),
      ("/tags/", "Tags"),
    ))
  })
}

#let site-footer = {
  let meta = ()
  meta.push(h("span")[Built with #link-to("/", "Baudelaire") v#build.at("version", default: "dev")])
  meta.push(h("span")[Typst #sys.version])
  if git != none {
    let short = git.hash.slice(0, 7)
    meta.push(h("span")[commit #html.a(href: "https://github.com/cestef/baudelaire/commit/"+git.hash)[#html.code[#short]]])
  }

  h("footer", class: "site-footer")[
    #h("div", class: "build-meta", meta.join())
    #h("div")[
      #link-to("/rss.xml", "RSS") ·
      #link-to("/atom.xml", "Atom") ·
      #link-to("/sitemap.xml", "Sitemap")
    ]
  ]
}

#let scripts = h("script", type: "module", src: "/assets/main.js")

#let callout(body, kind: "note", label: none) = h(
  "div",
  class: classes("callout", "callout-" + kind),
)[
  #h("p", class: "callout-label", if label != none { label } else { upper(kind) })
  #body
]

#let cards(items) = h("ul", class: "cards", for (href, icon, title, blurb) in items {
  h("li", h("a", href: href, {
    h("span", class: "card-icon", lucide(icon, size: 20))
    h("strong", title)
    h("span", blurb)
  }))
})

#let tag-row(tags) = h("div", class: "tag-row", aria-label: "Tags", for tag in tags {
  link-to("/tags/" + tag + "/", "#" + tag)
})

#let shell(title, main, tags: (), sections: ()) = {
  set document(title: title)
  show raw.where(lang: "kdl"): set raw(syntaxes: "/highlight/kdl.sublime-syntax")
  show raw: set raw(theme: "/highlight/baudelaire.tmTheme") // custom color mapping

  h("link", rel: "stylesheet", href: "/assets/style.css")
  h("link", rel: "icon", type: "image/svg+xml", href: "/assets/favicon.svg")
  site-header
  h("div", class: "layout")[
    #sidebar(sections)
    #h("main", class: "content", id: "main")[
      #h("article")[
        #h("h1", title)
        #main
        #if tags.len() > 0 { tag-row(tags) }
      ]
    ]
  ]
  site-footer
  scripts
}
