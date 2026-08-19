#let frontmatter = (
  title: "Authors and other entities",
  order: 11,
)
#import "/templates/theme.typ": callout

A taxonomy term is a word. An *entity* is a thing the build knows about: a
person with a homepage and an avatar, a series with a cover, a publisher with a
logo. Declare a registry, point a taxonomy at it, and the terms of that taxonomy
become ids in it.

```kdl
content {
  entities {
    people {
      shape "person"
      sources {
        inline {
          zoe { name "Zoe Quill"; url "https://zoe.example" }
        }
      }
    }
  }
  taxonomies {
    authors { entities "people"; credit "author" }
  }
}
```

A page writing `authors: ("zoe",)` is now credited to somebody the site knows,
and every surface that can name a person names them: the head tags, the JSON-LD
island, the feed entry, the card, and any template that reads `page.credits`.

#callout[
  Nothing here is about people. `people` is a name this site chose. A `series`
  registry with `display` bound to `title` and `image` to `cover` renders
  through the very same code.
]

== Sources

A registry's entities come from one or more sources, read in the order written.
A later source *fills* what an earlier one left out, so a roster can carry
contact details while profile pages carry the prose.

#table(
  columns: 3,
  align: (left, left, left),
  table.header([Source], [Writes], [Best for]),
  [`pages "content/people"`],
  [A directory of content pages: each page's frontmatter is one entity's fields, its slug the id.],
  [Bios. A profile is an ordinary page, so it has a permalink, an edition per language, and a card.],

  [`data "data/people.kdl"`],
  [A KDL file, one block per id, written exactly as `inline` is.],
  [A roster checked in beside the content, kept out of the config.],

  [`inline { .. }`],
  [Entities written in `config.kdl` itself.],
  [A handful of names that would be ceremony as a file.],
)

```kdl
//! content { entities { people {
sources {
  pages "content/people"
  data "data/people.kdl"
}
//! } } }
```

```kdl
//! @ignore
// data/people.kdl
zoe {
  name "Zoe Quill"
  url "https://zoe.example"
  alias "zq" "quill"
}
```

`alias` is a second name that resolves to the same entity: a former spelling, an
email address, a handle. A name that would reach two entities is a build error
naming both.

== Fields and slots

`shape` takes a named field set instead of declaring one. Two ship: `person`
(`name`, `url`, `avatar`, `email`, `socials`) and `organization` (`name`, `url`,
`logo`).

A registry's own `fields { }` fills in over the shape's, key by key, in the type
language a #link("../../configure/reference.typ")[collection schema] speaks.
Declaring a field *requires* it, which is how you make a roster complete:

```kdl
//! content { entities {
people {
  shape "person"
  fields {
    name "str"            // required, where the shape had it optional
    pronouns "str" optional=#true
  }
}
//! } }
```

*Slots* say which field answers each question a renderer asks. They are what
keeps the rest of the build from being written about people.

#table(
  columns: 3,
  align: (left, left, left),
  table.header([Slot], [`person` reads], [Rendered as]),
  [`display`], [`name`], [The name in every byline.],
  [`url`], [`url`], [`rel="author"`, `article:author`, the feed's `<uri>`.],
  [`image`], [`avatar`], [The JSON-LD `image`.],
  [`email`], [`email`], [The feed's `<email>`.],
  [`same-as`], [`socials`], [The JSON-LD `sameAs`, from a list or a dict of URLs.],
)

```kdl
//! content { entities {
series {
  fields { title "str"; cover "str" optional=#true }
  slots display="title" image="cover"
  sources { pages "content/series" }
}
//! } }
```

A slot naming a field the registry does not declare is refused at the line that
wrote it: unchecked, it would render every entity without its picture out of a
green build.

`display` falls back to the `name` field, then `title`, then the term itself, so
a profile page whose title *is* the person's name needs no `name` key.

== Credits

`credit` says what a page claims about the entities a taxonomy names. Each
surface spells the role its own way, or stays quiet where its vocabulary has no
word for it.

#table(
  columns: 5,
  align: (left, left, left, left, left),
  table.header([Role], [`<meta name>`], [OpenGraph], [JSON-LD], [Atom]),
  [`author`], [`author`], [`article:author`], [`author`], [`<author>`],
  [`contributor`], [--], [--], [`contributor`], [`<contributor>`],
  [`translator`], [--], [--], [`translator`], [--],
  [`editor`], [--], [--], [`editor`], [--],
  [`illustrator`], [--], [--], [`illustrator`], [--],
  [`reviewer`], [--], [--], [`reviewedBy`], [--],
  [`publisher`], [--], [--], [`publisher`], [--],
)

```kdl
//! content {
taxonomies {
  authors { entities "people"; credit "author" }
  translators { entities "people"; credit "translator" }
}
//! }
```

A taxonomy that names a registry without a `credit` is a plain reference: it
resolves and renders, and claims nothing about who made the page. That is what
`part-of entities="series"` is.

== In a template

`page.credits` is keyed by role, and only the roles the page names are there.
Each entity carries its slot answers under fixed names and its own fields under
`fields`.

```typ
#for one in page.credits.at("author", default: ()) [
  #link(one.url)[#one.name]
  #one.fields.pronouns
]
```

The same byline reaches a bundled document, so a PDF or a card renders the
authors its page does.

== A profile page as the term page

`describe=#true` means a term written as a page *is* that page. No listing is
generated beside it, the term index links to the profile's own permalink, and
the profile is handed everything credited to it as `page.members`, in the row
shape every listing carries.

```kdl
content {
  entities {
    people { shape "person"; sources { pages "content/people" } }
  }
  taxonomies {
    authors { entities "people"; credit "author"; describe #true; listing }
  }
}
```

```typ
#let profile(page, body) = {
  body                                     // the bio, as authored
  for one in page.members [ #link(one.url)[#one.label] ]
}
```

One URL for one person, so every link already written to the profile still
reaches it. A term nobody wrote a page for is generated as an ordinary term
listing.

== When a name is not in the roster

#table(
  columns: 2,
  align: (left, left),
  table.header([`unknown`], [Means]),
  [`error`], [Fail the build, suggesting the near id, underlined where the page wrote the term. The default once a registry has a source.],
  [`warn`], [Report it and carry on.],
  [`synthesize`], [Take the term as written: the name is all there is to know. The default for a registry with no source.],
)

A site that declares no registry at all keeps the byline it always had:
`author "Camille"` at the top level is the floor for every page, and a page's
own `author: "Zoe"` beats it. Both still work, and a registry always wins over
both.
