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

// The published documentation versions, newest first. Written by
// `docs/versions.sh` into the tree it is about to build, from the releases
// `CHANGELOG.md` declares and `git tag` confirms, so the picker offers exactly
// the versions that were deployed alongside it. The newest is what the site
// root serves, which is why it is the one labelled.
#let versions = csv("/generated/versions.csv").map(row => row.first())

// `page.url` arrives base-path prefixed, so a pinned build (`/v0.0.11/write/`)
// carries its own tag on the front. The picker composes prefixes itself and so
// needs the path without one.
#let _unpinned(url) = {
  let pinned = versions.find(tag => url == "/" + tag or url.starts-with("/" + tag + "/"))
  if pinned == none {
    url
  } else if url.len() == pinned.len() + 1 {
    "/"
  } else {
    url.slice(pinned.len() + 1)
  }
}

// Jump to this same page in another version. Plain links, no script: a version
// that never had this page answers with its own 404 rather than with a picker
// that silently refused to move.
#let version-picker(url) = {
  let here = if url == none { "/" } else { _unpinned(url) }
  h("details", class: "version-picker")[
    #h("summary", class: "version-current", {
      h("span", versions.first())
      lucide("chevron-right", size: 13)
    })
    #h("ul", class: "version-list", for (i, tag) in versions.enumerate() {
      h("li", link-to(
        if i == 0 { here } else { "/" + tag + here },
        if i == 0 { tag + " (latest)" } else { tag },
      ))
    })
  ]
}

#let site-header(url) = h("header", class: "site-header")[
  #h("a", class: "skip-link", href: "#main", "Skip to content")
  #h("a", class: "brand", href: "/", "Baudelaire")
  #h("nav", class: "top-nav", aria-label: "Primary")[
    #link-to("/start/install/", "Docs")
    #link-to("/configure/reference/", "Reference")
    #link-to("/start/themes/", "Themes")
    #link-to("/blog/", "Blog")
  ]
  #version-picker(url)
  #search-trigger
  #theme-toggle
  #nav-toggle
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
#let _doc-groups = (
  ("start", "Start"),
  ("write", "Write"),
  ("configure", "Configure"),
  ("build", "Build"),
  ("ship", "Ship"),
  ("lookup", "Look up"),
)

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

// Source beside output, from one string: the left pane is the snippet as
// written, the right pane is `eval` of that same string, compiled into this page
// by the same typst that compiled the rest of it. There is nothing to keep in
// sync and no screenshot to go stale.
//
// `variants` is `((tab, file, source), ..)`. Every variant is rendered at build
// time and shipped; the tab bar only swaps which one is shown, so a reader with
// no JavaScript still gets the first one, whole.
#let demo(variants) = h("div", class: "demo", data-demo: true)[
  #h("div",
    class: "demo-tabs",
    role: "tablist",
    aria-label: "Example",
    hidden: true,
    data-demo-tabs: true,
    for (i, (tab, ..)) in variants.enumerate() {
      h("button",
        type: "button",
        role: "tab",
        id: "demo-tab-" + str(i),
        class: classes("demo-tab", ("is-active", i == 0)),
        aria-selected: if i == 0 { "true" } else { "false" },
        aria-controls: "demo-pane-" + str(i),
        tabindex: if i == 0 { "0" } else { "-1" },
        data-demo-tab: str(i),
        tab)
    })
  #for (i, (_, file, src)) in variants.enumerate() {
    h("div",
      class: "panes",
      id: "demo-pane-" + str(i),
      role: "tabpanel",
      aria-labelledby: "demo-tab-" + str(i),
      hidden: i != 0,
      data-demo-pane: str(i),
      {
        h("div", class: "pane", {
          h("p", class: "pane-label", file)
          raw(src, lang: "typ", block: true)
        })
        h("div", class: "pane", {
          h("p", class: "pane-label", "what it renders")
          h("div", class: "demo-out", eval(src, mode: "markup"))
        })
      })
  }
]

// The `generate { }` block, as a thing you operate: one switch per artifact,
// laid out on the same grid as `cards`, over the config and the output tree the
// checked set adds up to.
//
// Each option carries the one config line it adds and the files that line
// writes, so the KDL and the tree come from this list rather than from two
// copies of it. `_home.js` reads the data attributes; with no script the
// switches ship disabled and the config shows the set they already display.
#let emit-explorer(options, block: "generate") = h(
  "div",
  class: "emit",
  data-emit: true,
)[
  #h("div", class: "toggles", for opt in options {
    let on = opt.at("on", default: false)
    // A `button` rather than a checkbox: the whole card is the control, it
    // needs no label to align against, and `role="switch"` says what it does.
    // Dead controls are worse than none, so it ships disabled and `_home.js`
    // enables it.
    h("button",
      type: "button",
      class: "toggle",
      role: "switch",
      aria-checked: if on { "true" } else { "false" },
      disabled: true,
      data-emit-option: opt.id,
      data-files: opt.at("files", default: ()).join(","),
      data-page-files: opt.at("page-files", default: ()).join(","),
      {
        h("span", class: "toggle-mark", aria-hidden: "true", lucide("check", size: 12))
        h("span", class: "toggle-title", opt.label)
        h("span", class: "toggle-note", opt.note)
      })
  })
  #h("div", class: "panes")[
    // Every option's line is highlighted at build time and shipped; toggling one
    // shows or hides its line rather than rewriting the block, so the KDL here is
    // always typst's own highlighting and never a string built in the browser.
    #h("div", class: "pane")[
      #h("p", class: "pane-label", "config.kdl")
      #h("pre", class: "emit-kdl", {
        h("span", class: "emit-line", raw(block + " {", lang: "kdl"))
        for opt in options {
          h("span",
            class: "emit-line",
            hidden: not opt.at("on", default: false),
            data-emit-line: opt.id,
            raw("  " + opt.kdl, lang: "kdl"))
        }
        h("span", class: "emit-line", raw("}", lang: "kdl"))
      })
    ]
    #h("div", class: "pane", hidden: true, data-emit-tree-pane: true)[
      #h("p", class: "pane-label", "public/")
      #h("div", class: "tree", data-emit-tree: true)
    ]
  ]
]

#let tag-row(tags) = h("div", class: "tag-row", aria-label: "Tags", for tag in tags {
  link-to("/tags/" + tag + "/", "#" + tag)
})

// The one page chrome. `sections: none` drops the sidebar and lets the article
// span the column; `heading: false` suppresses the generated `h1`, for a page
// that titles itself. The landing page is the only caller of either. `url` is
// the page's own, which only the version picker reads: without it a version
// jump lands on that version's home rather than on this page.
#let shell(title, main, tags: (), sections: (), heading: true, class: none, url: none) = {
  set document(title: title)
  show raw.where(lang: "kdl"): set raw(syntaxes: "/highlight/kdl.sublime-syntax")
  show raw.where(lang: "powershell"): set raw(syntaxes: "/highlight/powershell.sublime-syntax")

  h("link", rel: "stylesheet", href: "/assets/style.css")
  h("link", rel: "icon", type: "image/svg+xml", href: "/assets/favicon.svg")
  site-header(url)
  h("div", class: classes("layout", ("layout-full", sections == none)))[
    #if sections != none { sidebar(sections) }
    #h("main", class: "content", id: "main")[
      #h("article", class: class)[
        #if heading { h("h1", title) }
        #main
        #if tags.len() > 0 { tag-row(tags) }
      ]
    ]
  ]
  site-footer
  scripts
}
