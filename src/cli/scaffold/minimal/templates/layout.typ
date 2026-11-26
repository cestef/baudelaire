// The only template. A template file exports a function named after the file,
// so `layout.typ` exports `layout`; a page picks it with `template` in its
// frontmatter.
//
// It receives the page and its compiled `body`, and returns the markup for the
// document. typst-html supplies html/head/body around it, which is why nothing
// here emits those: a template that did would replace the generated head, and
// every meta tag Baudelaire appends to it would silently vanish.

#import "/.baudelaire/site/html.typ": h
#import "/.baudelaire/site/site.typ": title as site-title

// `h(tag, ..)` is `html.elem` without the ceremony: named arguments become
// attributes, positional ones become children.
#let layout(page, body) = {
  // typst-html turns the document title into <title>.
  set document(title: page.frontmatter.title)

  h("header", h("a", href: "/", site-title))
  h("main", h("article", {
    h("h1", page.frontmatter.title)
    body
  }))
}
