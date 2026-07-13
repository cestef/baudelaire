#let link-to(href, label) = html.elem("a", attrs: (href: href), label)

// Build metadata, injected by baudelaire at `sys.inputs.baudelaire`.
#let build = sys.inputs.at("baudelaire", default: (:))
#let git = build.at("git", default: none)

#let _icons = (
  search: (("circle", (cx: "11", cy: "11", r: "8")), ("path", (d: "m21 21-4.3-4.3"))),
  sun: (
    ("circle", (cx: "12", cy: "12", r: "4")),
    ("path", (d: "M12 2v2")), ("path", (d: "M12 20v2")),
    ("path", (d: "m4.93 4.93 1.41 1.41")), ("path", (d: "m17.66 17.66 1.41 1.41")),
    ("path", (d: "M2 12h2")), ("path", (d: "M20 12h2")),
    ("path", (d: "m6.34 17.66-1.41 1.41")), ("path", (d: "m19.07 4.93-1.41 1.41")),
  ),
  moon: (("path", (d: "M12 3a6 6 0 0 0 9 9 9 9 0 1 1-9-9Z")),),
  menu: (("path", (d: "M4 12h16")), ("path", (d: "M4 6h16")), ("path", (d: "M4 18h16"))),
  "arrow-right": (("path", (d: "M5 12h14")), ("path", (d: "m12 5 7 7-7 7"))),
  zap: (("path", (d: "M4 14a1 1 0 0 1-.78-1.63l9.9-10.2a.5.5 0 0 1 .86.46l-1.92 6.02A1 1 0 0 0 13 10h7a1 1 0 0 1 .78 1.63l-9.9 10.2a.5.5 0 0 1-.86-.46l1.92-6.02A1 1 0 0 0 11 14z")),),
  package: (
    ("path", (d: "M11 21.73a2 2 0 0 0 2 0l7-4A2 2 0 0 0 21 16V8a2 2 0 0 0-1-1.73l-7-4a2 2 0 0 0-2 0l-7 4A2 2 0 0 0 3 8v8a2 2 0 0 0 1 1.73z")),
    ("path", (d: "M12 22V12")), ("path", (d: "m3.3 7 8.7 5 8.7-5")), ("path", (d: "m7.5 4.27 9 5.15")),
  ),
  tag: (
    ("path", (d: "M12.586 2.586A2 2 0 0 0 11.172 2H4a2 2 0 0 0-2 2v7.172a2 2 0 0 0 .586 1.414l8.704 8.704a2.426 2.426 0 0 0 3.42 0l6.58-6.58a2.426 2.426 0 0 0 0-3.42z")),
    ("circle", (cx: "7.5", cy: "7.5", r: "1.5")),
  ),
  rss: (("path", (d: "M4 11a9 9 0 0 1 9 9")), ("path", (d: "M4 4a16 16 0 0 1 16 16")), ("circle", (cx: "5", cy: "19", r: "1"))),
  "sliders": (
    ("path", (d: "M4 21v-7")), ("path", (d: "M4 10V3")), ("path", (d: "M12 21v-9")),
    ("path", (d: "M12 8V3")), ("path", (d: "M20 21v-5")), ("path", (d: "M20 12V3")),
    ("path", (d: "M2 14h4")), ("path", (d: "M10 8h4")), ("path", (d: "M18 16h4")),
  ),
)

#let lucide(name, size: 18) = html.elem(
  "svg",
  attrs: (
    xmlns: "http://www.w3.org/2000/svg",
    width: str(size), height: str(size), viewBox: "0 0 24 24",
    fill: "none", stroke: "currentColor", "stroke-width": "2",
    "stroke-linecap": "round", "stroke-linejoin": "round",
    "aria-hidden": "true", class: "icon",
  ),
  {
    for (tag, attrs) in _icons.at(name) { html.elem(tag, attrs: attrs) }
  },
)

#let search-trigger = html.elem(
  "button",
  attrs: (
    class: "search-trigger",
    type: "button",
    "aria-label": "Search",
    "aria-keyshortcuts": "Control+K /",
    "data-search-open": "",
  ),
  {
    lucide("search", size: 16)
    html.elem("span", attrs: (class: "search-trigger-label"), "Search")
    html.elem("kbd", attrs: (class: "search-trigger-key"), "⌘K")
  },
)

#let theme-toggle = html.elem(
  "button",
  attrs: (
    class: "icon-btn",
    type: "button",
    "aria-label": "Toggle dark mode",
    "aria-pressed": "false",
    "data-theme-toggle": "",
  ),
  {
    html.elem("span", attrs: (class: "icon-moon", "aria-hidden": "true"), lucide("moon"))
    html.elem("span", attrs: (class: "icon-sun", "aria-hidden": "true"), lucide("sun"))
  },
)

#let nav-toggle = html.elem(
  "button",
  attrs: (
    class: "icon-btn nav-toggle",
    type: "button",
    "aria-label": "Toggle navigation",
    "aria-expanded": "false",
    "aria-controls": "sidebar",
    "data-nav-toggle": "",
  ),
  lucide("menu"),
)

#let site-header = html.elem("header", attrs: (class: "site-header"))[
  #html.elem("a", attrs: (class: "skip-link", href: "#main"), "Skip to content")
  #html.elem("a", attrs: (class: "brand", href: "/"), "Baudelaire")
  #html.elem("nav", attrs: (class: "top-nav", "aria-label": "Primary"))[
    #link-to("/guide/install/", "Guide")
    #link-to("/features/", "Features")
    #link-to("/blog/", "Blog")
  ]
  #search-trigger
  #theme-toggle
  #nav-toggle
]

#let nav-group(title, items) = html.elem("div", attrs: (class: "nav-group"))[
  #html.elem("p", attrs: (class: "nav-title"), title)
  #html.elem(
    "ul",
    for (href, label) in items {
      html.elem("li", link-to(href, label))
    },
  )
]

// The doc collections shown in the sidebar, as `(id, display title)`. Their
// pages — and their order — come from `page.sections` (the build's own view of
// the site), so the sidebar can never drift from the content or from the
// prev/next pager, which read the same source.
#let _doc-groups = (("guide", "Guide"), ("features", "Features"))

#let sidebar(sections) = {
  // `sections` is an array of `(id, pages: ((url, title), …))` in each
  // collection's sort order.
  let by-id = (:)
  for section in sections {
    by-id.insert(section.id, section.pages)
  }
  html.elem(
    "nav",
    attrs: (class: "sidebar", id: "sidebar", "aria-label": "Documentation"),
    {
      for (id, title) in _doc-groups {
        let pages = by-id.at(id, default: ())
        if pages.len() > 0 {
          nav-group(title, pages.map(p => (p.url, p.title)))
        }
      }
      nav-group("More", (
        ("/blog/", "Blog"),
        ("/tags/", "Tags"),
      ))
    },
  )
}

#let site-footer = {
  let meta = ()
  meta.push(html.elem("span")[Built with #link-to("/", "Baudelaire") v#build.at("version", default: "dev")])
  meta.push(html.elem("span")[Typst #sys.version])
  if git != none {
    let short = git.hash.slice(0, 7)
    meta.push(html.elem("span")[commit #html.a(href: "https://codeberg.org/cstef/baudelaire/commit/"+git.hash)[#html.code[#short]]])
  }

  html.elem("footer", attrs: (class: "site-footer"))[
    #html.elem("div", attrs: (class: "build-meta"), meta.join())
    #html.elem("div")[
      #link-to("/rss.xml", "RSS") ·
      #link-to("/atom.xml", "Atom") ·
      #link-to("/sitemap.xml", "Sitemap")
    ]
  ]
}

#let scripts = html.elem("script", attrs: (type: "module", src: "/assets/main.js"))

#let callout(body, kind: "note", label: none) = html.elem(
  "div",
  attrs: (class: "callout callout-" + kind),
)[
  #html.elem("p", attrs: (class: "callout-label"), if label != none { label } else { upper(kind) })
  #body
]

#let cards(items) = html.elem(
  "ul",
  attrs: (class: "cards"),
  for (href, icon, title, blurb) in items {
    html.elem("li", html.elem("a", attrs: (href: href), {
      html.elem("span", attrs: (class: "card-icon"), lucide(icon, size: 20))
      html.elem("strong", title)
      html.elem("span", blurb)
    }))
  },
)

#let tag-row(tags) = html.elem(
  "div",
  attrs: (class: "tag-row", "aria-label": "Tags"),
  for tag in tags {
    link-to("/tags/" + tag + "/", "#" + tag)
  },
)

#let shell(title, main, tags: (), sections: ()) = {
  set document(title: title)
  show raw.where(lang: "kdl"): set raw(syntaxes: "/highlight/kdl.sublime-syntax")
  show raw: set raw(theme: "/highlight/baudelaire.tmTheme") // custom color mapping

  html.elem("link", attrs: (rel: "stylesheet", href: "/assets/style.css"))
  html.elem("link", attrs: (rel: "icon", type: "image/svg+xml", href: "/assets/favicon.svg"))
  site-header
  html.elem("div", attrs: (class: "layout"))[
    #sidebar(sections)
    #html.elem("main", attrs: (class: "content", id: "main"))[
      #html.elem("article")[
        #html.elem("h1", title)
        #main
        #if tags.len() > 0 { tag-row(tags) }
      ]
    ]
  ]
  site-footer
  scripts
}
