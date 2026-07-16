#let frontmatter = (
  title: "A site generator has no business shipping a template language",
  date: datetime(year: 2026, month: 6, day: 20),
  tags: ("blog", "typst", "internals"),
  summary: "Every static site generator eventually grows a second programming language: Liquid, Tera, Handlebars, bolted onto a string templater by someone who didn't mean to design a language. Baudelaire doesn't, because Typst is already a language and the compiler is already the template engine.",
)
#import "/templates/theme.typ": callout

Pick a static site generator. Any of them. Now count the languages you have to
learn to use it. Markdown for the words. YAML for the frontmatter. And then, the
part nobody warns you about, a template language: Liquid, Handlebars, Tera, Go's
`text/template`, Nunjucks, take your pick. Three languages to turn a folder of
text into a folder of HTML.

That third one always bugged me. A template language is a programming language
wearing a fake moustache. It has variables, conditionals, loops, includes,
filters, sometimes even macros, but done badly, because nobody set out to design
a language. They set out to interpolate strings and kept saying yes. You want to
sort a list by a field? Hope there's a filter for it. You want to actually compute
something? Cram it into a `{% ... %}` block and pray.

Meanwhile Typst, a typesetting language ostensibly for making PDFs, already has
the whole thing. Real functions. Imports. Closures. Data loading. Math. And with
the `+html` feature, it emits HTML instead of pages.

So the entire premise of Baudelaire is: don't ship a fourth thing. A page is a
`.typ` file. A layout is a Typst function. There is no template language, because
the template language would just be a worse Typst.

== The compiler is the template engine

A layout looks like this:

```typ
#let post(page, body) = html.article[
  #html.h1(page.frontmatter.title)
  #body
]
```

No magic. `html.article` and `html.h1` are Typst's own typed element
constructors, one per HTML tag; `page` is a plain dict; `body` is the compiled
content. (Need a tag without a named constructor? `html.elem("tag", ..)` is the
escape hatch.) A layout is a function from data to markup. That's the whole
abstraction, and there isn't a second one hiding behind it.

The interesting question is how the page and the template actually meet. The
obvious way, the way every other generator does it, is to read the page, read the
template, and splice strings. Baudelaire splices nothing. It writes a tiny Typst
program and hands the whole thing back to the compiler:

```typ
#import "/templates/post.typ": post as __layout
#import "/content/blog/hello.typ": frontmatter as __data
#show: __body => __layout((frontmatter: __data, taxonomies: (tags: ("typst",)), nav: (prev: none, next: none), sections: ()), __body)
#include "/content/blog/hello.typ"
```

Look at what that does. The template function is just the file stem: `post.typ`
becomes `post`. The page's frontmatter arrives as an _import_ of its `frontmatter`
export, not a slab of parsed YAML. The page itself is `#include`-d, so it compiles
exactly as you wrote it, and a `#show` rule wraps its body in the layout. The real
page source is never touched. No string surgery, no regex, no "please don't put a
`---` inside a fenced code block."

#callout(kind: "note")[
  Why the awkward `__layout` and `__data` names? A layout is called with a
  variable named `page`. So if you name your template file `page.typ`, its
  function is _also_ called `page`, and the import would clash with that variable
  the moment the wrapper tried to use both. Importing under a mangled alias keeps
  the template, the page data, and anything you've named yourself from stepping on
  each other. It looks ugly precisely so your names never have to.
]

That generated wrapper gets its own synthetic `FileId`, a sibling of the page, so
relative imports inside the template resolve identically to how they would from
the page. But it's distinct enough that `#include`-ing the real file doesn't
quietly shadow it as `main`. This is Typst code generation, not HTML templating.
The output of a page is the output of compiling a program, and programs are a
thing I know how to reason about.

== Frontmatter is code, not a preamble

Because a page _is_ a program, its frontmatter is just a value the program
exports:

```typ
#let frontmatter = (
  title: "Hello",
  date: datetime(year: 2026, month: 6, day: 20),
  tags: ("typst",),
)
```

Baudelaire never scans for `---` fences and hands the slice to a YAML parser. It
evaluates the module and reads `frontmatter` straight out of the scope. Which
means your frontmatter can _compute_. `date: datetime.today()`. A title assembled
from a loaded CSV. A tag list built with a `for` loop. It's a binding in a real
language, so it does what bindings in real languages do.

The flip side is that I get to hand you real errors. `title` has to be a string,
`date` has to be a datetime, and if you write `tags: ("typst", 3)` you're told the
second element is wrong instead of getting a mystery failure three stages
downstream. One field table feeds both the parser and the did-you-mean suggester,
so `dat: 2026` gets you a "did you mean `date`?" instead of silently vanishing
into the `extra` junk drawer.

== What you get for free

Because it's Typst the whole way down, everything the compiler already does is
just... there. Math. Code highlighting. Imports. `json()` and `csv()` data
loading. Every package on Typst Universe. There is no plugin API, because the
plugin API is `import`.

What the generator adds is only the part a compiler leaves out: a content model
(collections, taxonomies, permalinks), the files a site needs (feeds, sitemap,
#link("../features/discovery/search.typ")[search]), and a
#link("../features/build/incremental.typ")[build cache] so this all stays fast.
Everything else is Typst.

This site is written this way. So is my actual blog. The manifesto fits on a
napkin: don't invent a language when you're already standing on one. Read the
#link("../guide/quickstart.typ")[quickstart] if you want to try it.
