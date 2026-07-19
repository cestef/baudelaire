#let frontmatter = (
  order: 5,
  title: "Internationalization",
  tags: ("feature", "content", "i18n"),
)
#import "/templates/theme.typ": callout

Baudelaire builds a multi-language site from one content tree. Declare your
languages, drop translated files next to their originals, and every page,
listing, feed, and meta tag localizes itself.

== Declare the languages

The default language is `lang`; a `languages` block adds the rest. Each entry is
keyed by a language code and may carry a display name, a writing direction, and a
UI-string table:

```kdl
lang "en"

languages {
  fr {
    name "Français"
    strings { read-more "Lire la suite" }
  }
  ar { name "العربية"; dir "rtl" }
}
```

Without a `languages` block a site is single-language and nothing below applies.

== Translate a page

A translation is a sibling file with a `.{code}.typ` suffix, the same convention
as `.draft.typ`:

```
content/
  index.typ        // default language (en)
  index.fr.typ     // French
  posts/
    hello.typ
    hello.fr.typ
```

The default language keeps clean root URLs; every other language sits under
`/{code}/`:

- `content/index.typ` renders at `/`
- `content/index.fr.typ` renders at `/fr/`
- `content/posts/hello.fr.typ` renders at `/fr/posts/hello/`

A page with no translation in a language is simply omitted for it, so a language
only shows the pages you actually translated. A frontmatter `lang` overrides the
filename, for when a suffix doesn't fit:

```typ
#let frontmatter = (title: "Über", slug: "about", lang: "de")
```

#callout(kind: "note")[
  Translations are matched by collection and `slug`: `hello.typ` and
  `hello.fr.typ` share the slug `hello`, so they are linked. Give a translation a
  *different* slug and it becomes a standalone page instead. An unknown `lang`
  (one you never declared) is a hard error, never a silent mystery URL.
]

== Everything localizes

Once languages are declared, the rest of the build follows automatically:

/ Taxonomies: terms never merge across languages. A French and an English `rust`
  tag are separate `/fr/tags/rust/` and `/tags/rust/` pages.
/ Listings and pagination: each language paginates its own collection under its
  own prefix (`/blog/`, `/fr/blog/`).
/ Navigation: `page.sections` and prev/next only ever reference same-language
  pages.
/ Feeds: one set per language, the default at `/rss.xml`, others under
  `/{code}/rss.xml`, each listing only its language's posts.
/ Meta: `<html lang>` (and `dir` for a right-to-left language), `og:locale`, and
  `hreflang` alternates (plus `x-default`) in every `<head>` and in
  `sitemap.xml`, so crawlers pair the translations.

== Build a switcher

Every page receives its language, its editions, and the current language's
strings:

```typ
#let layout(page, body) = {
  // the current language
  text(page.lang)
  // a link to each translation, current one marked
  for t in page.translations {
    let label = if t.lang == page.lang [*#t.lang*] else [#t.lang]
    link(t.url, label)
  }
  // a UI string from the language's `strings` table
  page.strings.at("read-more", default: "Read more")
  body
}
```

`page.translations` includes the page's own edition, so a switcher can render the
full set and mark the active one by `page.lang`. The declared languages are also
at `sys.inputs.baudelaire.site.languages` as `(code, name)` pairs.

== Client-side

The `baudelaire:i18n` #link("../assets/js-modules.typ")[virtual module] exposes
the language list and every string table to bundled JavaScript, for a client
switcher or localized UI text:

```js
import { languages, strings } from "baudelaire:i18n";

for (const { code, name } of languages) addOption(code, name);
element.textContent = strings.fr["read-more"];
```
