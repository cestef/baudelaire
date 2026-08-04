// The shared page layout. A template file exports a function named after the
// file, so `layout.typ` exports `layout`. `content { template }` in config.kdl
// binds it site-wide; a collection or a page's own frontmatter can name another.
//
// It receives the page and its compiled `body`, and returns the markup for the
// document. typst-html supplies html/head/body around it, which is why nothing
// here emits those: a template that did would replace the generated head, and
// every meta tag Baudelaire appends to it would silently vanish.

#import "@baudelaire/html:0.1.0": h
#import "@baudelaire/site:0.1.0": author, title as site-title

// `h(tag, ..)` is `html.elem` without the ceremony: named arguments become
// attributes, positional ones become children, and `none` drops an attribute
// instead of writing an empty one.

// A frontmatter `date` is a real typst datetime, so it is formatted here rather
// than written out by hand in every post.
#let posted(date) = if date != none {
  h("p", class: "meta", h(
    "time",
    datetime: date.display("[year]-[month]-[day]"),
    date.display("[month repr:long] [day padding:none], [year]"),
  ))
}

// `page.taxonomies` is the parsed view of the taxonomies config: a dict of
// taxonomy name to the terms this page carries. Reading it beats guessing which
// frontmatter keys happen to be taxonomies.
#let chips(terms) = if terms.len() > 0 {
  h("nav", class: "chips", aria-label: "Tags", for term in terms {
    h("a", class: "chip", href: "/tags/" + term + "/")[\##term]
  })
}

// `page.nav` is the previous and next page in the collection's own order, so on
// a reverse-dated blog `prev` is the newer post.
#let pager(nav) = if nav.prev != none or nav.next != none {
  h("nav", class: "pager", aria-label: "Post navigation", {
    if nav.prev != none {
      h("a", rel: "prev", href: nav.prev.url)[← #nav.prev.title]
    } else {
      h("span")
    }
    if nav.next != none {
      h("a", rel: "next", href: nav.next.url)[#nav.next.title →]
    }
  })
}

#let layout(page, body) = {
  let fm = page.frontmatter
  // typst-html turns the document title into <title>, so this is how a page
  // names itself; the meta tags read the same value.
  set document(title: fm.title)

  // A template cannot reach <head>, so its resource links go here, at the top
  // of the body. Browsers accept that, and Baudelaire lifts them back into the
  // head when it assembles a single-file export.
  h("link", rel: "stylesheet", href: "/assets/style.css")
  h(
    "link",
    rel: "alternate",
    type: "application/rss+xml",
    title: site-title,
    href: "/rss.xml",
  )

  h("header", class: "site-header", {
    h("a", class: "brand", href: "/", site-title)
    h("nav", class: "site-nav", {
      h("a", href: "/posts/")[Posts]
      h("a", href: "/tags/")[Tags]
    })
  })

  h("main", h("article", {
    h("h1", fm.title)
    posted(fm.at("date", default: none))
    body
    chips(page.taxonomies.at("tags", default: ()))
    pager(page.nav)
  }))

  h("footer", class: "site-footer", {
    // `author` is `none` when config.kdl leaves it unset, so every site
    // identity binding is worth guarding before it reaches the page.
    if author not in (none, "") { h("span", author) }
    h("a", href: "/rss.xml")[RSS]
    h("a", href: "/atom.xml")[Atom]
  })
}
