#let frontmatter = (
  title: "Multiple languages",
  order: 7,
)
#import "/templates/theme.typ": callout

One content tree, many languages. Declare them, drop translated files next to their originals, and pages, listings, feeds, and meta tags localize themselves.

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

Without a `languages` block the site is single-language and nothing below applies.

#table(
  columns: 3,
  align: (left, left, left),
  table.header([Key], [Type], [Does]),
  [`name`], [str], [The language's name in its own language, for a switcher.],
  [`dir`], [str], [Writing direction, `ltr` or `rtl`.],
  [`site`], [str], [The site name in this language.],
  [`author`], [str], [The default author in this language.],
  [`strings`], [key=value], [This language's UI string table.],
)

== Translate a page

A translation is a sibling file with a `.{code}.typ` suffix, the same convention as `.draft.typ`:

```text
content/
  index.typ        // default language (en)
  index.fr.typ     // French
  posts/
    hello.typ
    hello.fr.typ
```

The default language keeps clean root URLs. Every other language sits under `/{code}/`:

#table(
  columns: 2,
  align: (left, left),
  table.header([File], [URL]),
  [`content/index.typ`], [`/`],
  [`content/index.fr.typ`], [`/fr/`],
  [`content/posts/hello.fr.typ`], [`/fr/posts/hello/`],
)

A page with no translation in a language is omitted for that language, so a language only shows what you actually translated. A frontmatter `lang` overrides the filename when a suffix does not fit:

```typ
#let frontmatter = (title: "Über", slug: "about", lang: "de")
```

#callout(kind: "warn")[
  A `lang` you never declared is a hard error, and so is a filename suffix that looks like a language code (two or three lowercase letters) but is not declared. A mystery URL is the worse outcome.
]

== Localized slugs

Editions pair on their slug, so renaming one would strand it. Name the pairing with `translation` instead and each edition takes the slug its readers expect:

```typ
// content/posts/hello.typ
#let frontmatter = (title: "Hello", translation: "greeting")

// content/posts/bonjour.fr.typ
#let frontmatter = (title: "Bonjour", translation: "greeting")
```

`/posts/hello/` and `/fr/posts/bonjour/` now link to each other. The key is yours to pick and only has to match. It never appears in a URL.

== What localizes on its own

- #link("collections/taxonomies.typ")[Taxonomies]: terms never merge across languages. A French `rust` tag is `/fr/tags/rust/`, separate from `/tags/rust/`.
- #link("collections/pagination.typ")[Listings]: each language paginates its own collection under its own prefix.
- #link("collections/navigation.typ")[Navigation]: prev/next and the section tree only ever reference same-language pages.
- #link("../build/generate/feeds.typ")[Feeds]: one set per language, the default at `/rss.xml`, the rest under `/{code}/rss.xml`.
- #link("../build/generate/meta.typ")[Meta]: `<html lang>` and `dir`, `og:locale`, and `hreflang` alternates (plus `x-default`) in every head and in `sitemap.xml`.

== A language switcher

```typ
#let page(page, body) = {
  for t in page.translations {
    let label = if t.lang == page.lang [*#t.lang*] else [#t.lang]
    link(t.url, label)
  }
  page.strings.at("read-more", default: "Read more")
  body
}
```

`page.translations` is an array of `(lang, url, title)` including the page's own edition, so a switcher renders the full set and marks the current one by `page.lang`. `page.strings` is the current language's table. See #link("templates.typ")[templates] for the rest of the page object.

The declared languages are also exported by `@baudelaire/site` as `languages`, a list of `(code, name)`.

== Dates

ISO-8601 is right for a machine and wrong for a reader, and Typst cannot fix it in a template: `datetime.display` knows English month names only. Every page and every listing row carries the date twice.

```typ
html.elem("time", attrs: (datetime: page.date.iso), page.date.display)
```

`page.date.iso` is `2026-07-30`; `page.date.display` is what the language writes. A listing row carries the same pair as `entry.date` and `entry.display`. A page with no date has `page.date` as `none`.

Two `strings` keys shape the display form, so a language declares its own with no locale database in the binary:

```kdl
languages {
  fr {
    strings {
      date "{day} {month} {year}"
      months "janvier" "février" "mars" "avril" "mai" "juin" \
             "juillet" "août" "septembre" "octobre" "novembre" "décembre"
    }
  }
}
```

`date` takes `{day}`, `{day2}` (zero-padded), `{month}` and `{year}`. The default is `{month} {day}, {year}`. A language that names no `months`, or names the wrong number of them, falls back to the English name.

== Client-side

The `baudelaire:i18n` #link("../lookup/js-modules.typ")[virtual module] hands the language list and every string table to bundled JavaScript:

```js
import { languages, strings } from "baudelaire:i18n";

for (const { code, name, dir } of languages) addOption(code, name);
element.textContent = strings.fr["read-more"];
```
