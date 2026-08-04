# Changelog

Notable changes to baudelaire. Format follows [Keep a Changelog][kac]; versions
follow [Semantic Versioning][semver], with the pre-1.0 caveat that a breaking
change bumps the patch number.

Nothing that only affects the repository is listed: refactors, tests, CI and
chores are visible in the git history and change nothing for a site.

[kac]: https://keepachangelog.com/en/1.1.0/
[semver]: https://semver.org/spec/v2.0.0.html

## [Unreleased]

### Added

- **generate**: `feed { names }`, what each format's file is called.

  ```kdl
  generate {
    feed {
      formats "rss"
      names { rss "index.xml" }   // Hugo's name; Jekyll's is feed.xml
    }
  }
  ```

  The file the build writes, the `<id>` the feed claims for itself, and every
  page's autodiscovery tag follow the name together. It exists for a site moving
  here from a generator that named the file differently: a feed is the one URL a
  redirect stub cannot rescue, because a reader fetches the file rather than
  rendering its meta refresh. A format with no override keeps the conventional
  name.

- **config**: `description`, what the site is in one line, with a per-language
  override beside `site` and `author`.

  ```kdl
  site "Fernweh"
  description "Notes from the road."

  languages {
    fr { description "Notes de voyage." }
  }
  ```

  It fills RSS's mandatory channel `<description>`, Atom's `<subtitle>` and JSON
  Feed's `description`, and `generate { llms { summary } }` falls back to it.
  Every feed used to repeat its own title there, which readers show twice and
  validators flag. It is deliberately *not* a fallback for a page's
  `<meta name="description">`: one sentence on every page is duplicate metadata.
  Templates read it as `description` from `@baudelaire/site`.

- **generate**: `manifest.webmanifest`, the web app manifest a browser reads to
  install the site to a home screen. The block's presence writes it, one per
  language, and every page gains a `<link rel="manifest">` pointing at its own
  language's (plus a `theme-color` meta tag when `theme` is set).

  ```kdl
  generate {
    manifest {
      short "Baudelaire"
      display "standalone"
      theme "#101014"
      icons {
        "/icons/app-192.png" size=192
        "/icons/app-512.png" size=512 purpose="maskable"
      }
    }
  }
  ```

  What the build knows it fills in: `name` from `site`, `start` and `scope` from
  where that language's site begins, an icon's media type from its extension,
  and a base path onto every URL it writes. An authored `start`/`scope` is
  localized like the default it replaces, so `start "/home/"` launches the
  French app into `/fr/home/` rather than out of its own scope. An icon with no `size` declares
  `any`, which is what a vector icon actually offers.

  A manifest with no icons is still written, and nothing will ever offer to
  install it, so the build warns (`baudelaire::manifest::icons`).

- **links**: backlinks. `links { backlinks #true }` hands every page the pages
  whose content links to it, as `page.backlinks`, each entry `(url, title, lang,
  fragments)` and ordered by URL. A link written as a URL (`#link("/guide/")`)
  counts alongside the `.typ` spelling; a generated index is not a source, or
  every page it lists would be backlinked from it.

  ```kdl
  links {
    backlinks #true
  }
  ```

  ```typ
  #let post(page, body) = {
    body
    h("ul", for l in page.backlinks { h("li", h("a", href: l.url, l.title)) })
  }
  ```

  `fragments` holds the heading ids the linking page aimed at, so a template can
  group its backlinks by section:

  ```typ
  #let cited(page, id) = page.backlinks.filter(l => id in l.fragments)
  ```

  Only links an author wrote in the content tree count: a layout's nav, the
  prev/next pair and a generated listing's index are links a page carries by
  virtue of its template, and counting them would make every page a backlink of
  every other. A page that links here three times is one entry carrying every
  section it named, and a page never backlinks itself.

  The pages linking to a page are not knowable until every page has rendered, so
  a build compiles against a guess (the graph the last build recorded, or on a
  first build the links each source writes out literally) and compiles again
  only the pages that turn out to disagree with the site. An edit that changes
  no links repairs nothing; adding a link recompiles the page it points at.
  Social cards and PDFs are not redrawn by that second compile and carry no
  backlinks.

  Content whose *own links* depend on its backlinks never settles: the build
  stops after the second attempt and warns
  (`baudelaire::backlinks::unstable`).

- **links**: `links { orphans "any" }` reports the pages nothing links to;
  `orphans "authored"` reports the pages nobody *wrote* about.

  ```
  ⚠ 2 pages linked from nowhere
    ⚠ `guide/exporting.typ` is linked from nowhere, and serves at `/guide/exporting/`
  ```

  A link counts when an author wrote it, spelled as a `.typ` path or as a URL. A
  layout never does: a sidebar links every page from every page. The mode decides
  whether the build's own listings count: under `any` a paginated index and a
  term page are ways in, so the report names only pages a reader cannot get to;
  under `authored` they are not, which names a post reached from its index and
  from nowhere else. The root of each language, the listings themselves and the
  not-found page are left out of both.

  A listing's entries are read from the page set, not from its markup, so a
  listing with a template of its own counts like the default one.

  A report, never a failure. Either switch turns the link graph on, so a site
  that wants only the report pays for the edges and none of the second compiles.

- **cli**: `baudelaire theme`. The four shipped themes are carried in the binary,
  so adopting one is a command rather than a clone of this repository:

  ```sh
  baudelaire theme list          # the four, one line each
  baudelaire theme add albatros  # writes themes/albatros/, then names the config line
  ```

  `add` never overwrites a file already there, so a second run over a theme you
  have edited changes nothing. `--dir` writes it elsewhere inside the project
  root. `init --theme "themes/albatros"` writes it for you as part of the
  scaffold. The `themes` cargo feature carries them (~230 KiB); a `slim` binary
  has neither the themes nor the command.

- **build**: a template nothing supplies is one typed diagnostic
  (`baudelaire::template::missing`) naming the file, what asked for it (a config
  key, or the page whose frontmatter named it) and where to write it, raised
  before the first compile. It used to be typst's own `file not found`, once per
  page, pointing at the generated wrapper.

- **init**: every scaffold writes a `.gitignore` for `public/` and `.baudelaire/`,
  not only the runs that set up a repository with `--vcs`.

### Fixed

- **redirects**: a site that publishes its own `static/_redirects` keeps its
  declared redirects. `generate { redirects }` writes a rule file *instead of*
  stubs, and a static file wins any path it claims, so both halves used to
  disappear at once: the generated rules were dropped on the way out and the
  stubs were never written, leaving every declared old path a 404 with nothing
  said. The stubs are written instead, and the build reports which mechanism it
  used (`baudelaire::output::redirects_shadowed`).

- **redirects**: a translated page's `redirect` entries are checked under their
  own language, as they are already written. Translating a page by copying its
  frontmatter carries the list along, and each edition forwards the old path
  under its own prefix (`/old/a/` and `/fr/old/a/`, two files). The collision
  check compared them unlocalized, so the documented translation workflow failed
  the build over a clash that never existed on disk.

- **listings**: a row carries `description`, resolved from a page's
  `description` or its `summary` alias, and every shipped template reads it. A
  listing used to reach into `entry.extra.summary` itself, so a site that wrote
  `description`, the spelling the docs teach and the one that fills the meta tag
  and the feed, got a blank preview under every entry. The scaffolds and themes
  that hardcoded the alias are updated, and the same field is on
  `@baudelaire/pages` and `baudelaire:pages`.

- **init**: `--theme` scaffolds a project the theme can render. It wrote a
  starter shape's whole config over the theme: a `collections` list (which
  replaces a theme's rather than merging), and `template` keys naming layouts
  the theme does not ship, so the documented flow failed its first build with a
  typst `file not found` per page. The themed scaffold now states only what a
  theme cannot decide: the site's identity, its paths, and a preview. `-t` is
  not used with `--theme`, and says so.

- **init**: `--with` skips a feature the starter shape already configures.
  `init -t docs --with search` appended a second, barer `generate { search }`
  block beneath the one the shape had written.

- **images**: a picture that lives in the asset tree and is shown by a page
  (`#image("/assets/photo.png")`) is referenced where the pipeline serves it
  instead of being extracted a second time. The extra copy claimed the same
  filename, warned that two images mapped to it, and could serve the
  unprocessed source in place of the optimized file.

- **themes**: `albatros` and `spleen` bind a layout for the pages directly under
  `content/`, which `phares` and `paysage` already did. A home page under either
  of them rendered as bare markup, with none of the theme's chrome.

- **build**: `assets { minify }` no longer warns that it is inert without
  `assets { bundle }`. It minifies stylesheets on its own, which is all a site
  with no JavaScript asked for; the warning fired on every such site, the `docs`
  starter included, and `--strict` failed it.

- **links**: the broken-link diagnostic suggests `--no-strict-links`, which
  exists, rather than `--strict-links false`, which the CLI refuses.

- **cli**: `clean --help` describes `--output` without a digression about this
  repository's own docs site.

### Performance

- **images**: responsive variants go through `optimize` like the file they were
  cut from. A downscaled PNG was written as the encoder produced it, so a
  `srcset` could offer a 960px candidate several times the weight of the
  optimized full-size image beside it.

- **images**: an image extracted from a page (the page-bundle layout, a photo
  beside the `.typ` that shows it) goes through the whole pipeline: the same
  optimizer, the same `responsive` variants, the same cross-build memo. It was
  copied byte for byte, so it was both the one unrecompressed file on the site
  and the one `<img>` with no `srcset`.

  ```typ
  #image("photo.png")
  ```

  ```html
  <img src="/assets/photo.png"
       srcset="/assets/photo-480.png 480w, /assets/photo.png 1600w"
       sizes="(min-width: 60rem) 640px, 100vw">
  ```

  The page names the variants from the source's own width before they exist, and
  the copy that materializes the image cuts exactly those widths. A fingerprinted
  variant carries the source's digest, since the two change together and the page
  names the file first.

- **links**: a cold build guesses each page's backlinks by reading the `.typ`
  links its source writes out, rather than assuming nothing links anywhere. On
  a 1,000-page site where every link is written by hand, the first build now
  compiles each page once instead of compiling 969 of them twice: 513 ms to
  403 ms before the other two changes below.

- **search**: the inverted index is built across the thread pool. Tokenizing
  every word of every page was the slowest thing a build did outside the
  compiles: on a 1,000-page site it was 113 ms of a 265 ms rebuild, and is now
  under 10 ms. The index itself is unchanged.

- **build**: each page's compile input (its wrapper text and the fingerprint
  that validates it) is prepared across the pool rather than one page at a time
  ahead of the compiles. Same site: a rebuild that reused every page went from
  265 ms to 150 ms with the change above.

### Upgrading

- The build cache records three more things per page: the links that page's own
  content carries, the digest of the backlinks it was compiled with, and the
  responsive widths each extracted image was promised. A manifest written before
  this records none of them, so the cache schema is bumped and the first build
  after upgrading is a cold one. Nothing to change.

- A template a config or a page names and nothing supplies now fails the build
  with `baudelaire::template::missing`. Such a build already failed, in typst's
  words rather than baudelaire's, so nothing that built before this fails now.

- `baudelaire new` writes no `template` key when the config binds no layout for
  the page's collection, where it used to write `template: "layout.typ"` on
  faith. The four starter shapes bind theirs under `_root`, the collection a
  page directly under `content/` lands in; a project that relied on the old
  default can state the same:

  ```kdl
  content {
    collections {
      _root { template "layout.typ" }
    }
  }
  ```

## [0.0.10] - 2026-08-03

### Added

- **assets**: TypeScript and JSX entry points. `bundle` now claims `.ts`,
  `.mts`, `.cts`, `.tsx`, `.jsx` and `.cjs` alongside `.js` and `.mjs`; types
  are stripped, JSX is transformed, and the result is served as `.js` under
  either spelling. An extension left out of that list used to fall through to
  the verbatim copy, so a `.tsx` entry shipped its unstripped source to the
  browser.

  Nothing is type-checked: the bundler transforms, as esbuild and Vite do.

- **assets**: `assets { tsconfig }` pins the `tsconfig.json` TypeScript and JSX
  are transformed against, for `paths` aliases, `jsxImportSource` and the rest.

  ```kdl
  assets {
    bundle #true
    tsconfig "tsconfig.json"
  }
  ```

  The path is relative to the project root, and a path with nothing at it fails
  the build (`baudelaire::asset::tsconfig`). Unset, one is discovered per script
  by walking up from the file, as `tsc` does. A pinned file is not a watched
  source: name it in `serve { include }` for the dev server to see edits to it.

- **content**: a collection's `schema` types nest. A list names what it holds
  (`list<int>`, `list<list<int>>`), a `dict` field declares its own fields in a
  block, and the two compose as `list<dict>`:

  ```kdl
  content {
    collections {
      blog {
        schema {
          widths "list<int>"
          authors "list<dict>" {
            name "str"
            email "str" optional=#true
          }
        }
      }
    }
  }
  ```

  The block declares the fields of the dictionary the type ends in, through
  however many lists wrap it, and nested fields are required unless they say
  `optional=#true` themselves. A failure names the field that broke down to the
  element (`authors.1.name`) and underlines it in the page. Bare `list` still
  means `list<str>`, so nothing written before this changes meaning.

- **install**: `install.ps1`, the Windows counterpart of `install.sh`. Same
  knobs under the same names (as environment variables: `$env:VERSION`,
  `$env:PREFIX`, `$env:FLAVOR`), same steps, same checksum promise. It resolves
  the latest release, downloads `baudelaire-windows-x86_64.zip`, verifies its
  `sha256`, and installs to `%LOCALAPPDATA%\Programs\baudelaire`. Like
  `install.sh` it never edits `PATH`, printing the line that would instead.

  ```powershell
  irm https://baudelaire.cstef.dev/install.ps1 -OutFile install.ps1
  .\install.ps1
  ```

  Windows on ARM is refused by name rather than served the `x86_64` build to run
  under emulation.

- **mirror**: `baudelaire mirror` (`packages`, `pkg`) writes every generated
  module where an editor resolves it: the `@baudelaire/*` typst modules as
  ordinary packages, and the `baudelaire:*` JavaScript modules as one TypeScript
  declaration file in the project. `init` runs it for a fresh project.

  ```sh
  baudelaire mirror                         # both, default locations
  baudelaire mirror --path .typst-packages  # typst packages somewhere else
  baudelaire mirror --uninstall             # take it all back off
  ```

  Both land in the project, under `.baudelaire/generated/`: three of the four
  typst modules describe *this* site (`site` from its config, `sections` and
  `pages` from its pages), so one machine-global copy would show one project's
  data to every other project's editor. The price is one setting per family, and
  the run closes on exactly those, ready to paste:

  ```
  ◆ editor setup
  ➜ typst     --package-path /abs/path/.baudelaire/generated/packages
    ↳ or TYPST_PACKAGE_PATH; tinymist takes it in typstExtraArgs
  ➜ tsconfig  add .baudelaire/generated/baudelaire.d.ts to the include list
  ```

  `init` prints the same block once the project is written. `-v` lists every
  module the run wrote. `--global` writes typst's own package directory instead,
  where they resolve with nothing configured and there is no typst setting to
  make, and `--path <dir>` names a third place. `--uninstall` takes back exactly
  what a run wrote.

  A build never reads any of it, so a stale copy cannot change a page: the
  compiler answers `@baudelaire/*` from memory before typst's package
  resolution runs. Re-run it after upgrading baudelaire.

- **types**: every build writes `.baudelaire/generated/baudelaire.d.ts`, typing
  the `baudelaire:*` modules a bundled entry imports. Put it on the `include`
  list:

  ```json
  { "include": ["assets/**/*.ts", ".baudelaire/generated/baudelaire.d.ts"] }
  ```

  `site`, `config` and `i18n` are typed from the site's own values, so a
  `client { }` constant carries the type the config gives it rather than
  `unknown`. `baudelaire mirror` writes the same file, for a checkout that has
  not been built yet.

### Fixed

- **assets**: a `.d.ts` in the asset tree is read for its types and no longer
  bundled as an entry of its own, which wrote an empty `.d.js` beside it.

- **typst modules**: a *content* page can import `@baudelaire/pages` (or
  `@baudelaire/sections`). It used to fail the build on a first run in a fresh
  checkout, with `file not found (searched at .baudelaire/generated/pages.typ)`,
  and to silently serve the previous build's table afterwards. Frontmatter
  discovery evaluates a page whole, so the import lands before the build has
  written the table; that read now answers the empty table every module already
  promises for a language that was not built, and the page's own compile reads
  the table this build wrote.

  ```typ
  #import "@baudelaire/site:0.1.0": lang
  #import "@baudelaire/pages:0.1.0": pages

  This site has #pages(lang).len() pages.
  ```

  A `title`, `slug`, or `date` computed from `pages()` still gets nothing: the
  catalogue is built from the frontmatter being collected, so it is empty at
  that moment and no ordering can change it.

- **reference**: three key descriptions in `baudelaire reference` (and the
  generated config reference) described something the build does not do.
  `content { draft { suffix } }` is a filename marker peeled off the stem, not
  something appended to a URL; `announce { standard { discover } }` opts the
  publication into standard.site's discovery surfaces rather than resolving the
  PDS from the handle; and `announce { standard { verify { wellknown } } }`
  writes `/.well-known/site.standard.publication`, not `/.well-known/atproto-did`.

### Upgrading

With `assets { bundle #true }`, a `.ts`, `.mts`, `.cts`, `.tsx`, `.jsx` or
`.cjs` file in the asset tree is now an entry the bundler owns, where it used to
fall through to the verbatim copy. Two things follow for a site that already had
one.

It has to parse. A file the bundler cannot read now fails a build that used to
copy it and ship it, unstripped, to the browser.

It is served as `.js`. A `src` or `href` in a template follows it, since both
spellings are mapped, but a URL assembled at runtime in client JS, or linked
from outside the site, still names the old path.

Nothing changes with bundling off, which is the default: JavaScript and
TypeScript alike are copied as they are written.

A schema field's `{ }` block used to be read and thrown away; it now declares
the fields of a `dict`, so a block on a type that holds no dictionary
(`hero "str" { .. }`) fails the config rather than being ignored.

Frontmatter is judged against the schema when a page is discovered, and the
cache that skips that work keys on the schema, whose shape changed. The first
build after upgrading re-reads every page's frontmatter. Nothing to do.

## [0.0.9] - 2026-08-01

### Breaking

- **themes**: `voyage` is gone, and the shipped set is now one theme per kind of
  site: `albatros` (a blog), `spleen` (a blog with no JavaScript), `phares`
  (documentation), `paysage` (a portfolio). Everything voyage did is in
  `albatros`, which now builds a language switcher from each page's own editions
  and reads every visible word from the site's string table, so the migration is
  one line:

  ```kdl
  theme "themes/albatros"
  ```

  A site that wants voyage's serif look back can keep its own copy: the last
  version of it is in the `v0.0.8` tag under `themes/voyage/`.

- **js**: A `baudelaire:pages` row is now the same shape a generated listing
  hands its template: `{ url, label, collection, lang, date, display, note,
  taxonomies, extra }`. `title` was renamed to `label`, and the row gained the
  localized `display` date and the page's own remaining frontmatter as `extra`.
  In client code, `p.title` becomes `p.label`; nothing else moved.

### Added

- **content**: `collections { <id> { schema { } } }`, a frontmatter schema per
  collection. One line per field, the field's name and the type it must hold
  (`str`, `bool`, `int`, `float`, `date`, `list`, `any`):

  ```kdl
  content {
    collections {
      blog {
        schema {
          title "str"
          tags "list"
          hero "str" optional=#true
        }
      }
    }
  }
  ```

  Declaring a field requires it; `optional=#true` lets it be absent, and a field
  written bare (`author`) is required but unconstrained. A page that omits a
  required field or writes the wrong type fails the build with the offending
  frontmatter line underlined, instead of a template silently rendering nothing.
  A recognized key can be required too, but its type is already fixed by the
  build: declaring a different one is a config error. A collection with no
  `schema` block constrains nothing, which stays the default.

- **check**: `lint { }`, a linter over the typed DOM. Four rules, each a flag and
  all on while the block is present: `headings` (a level skipped, `h2` straight
  to `h4`), `alt` (an image with no text alternative; an empty `alt` is a
  deliberate "decorative" and passes), `ids` (an id used twice, which silently
  breaks every deep link into it), and `aria` (a role that is not a role, an
  `aria-*` attribute ARIA does not define, and an `aria-labelledby` naming an id
  that is not on the page). Findings are warnings; `lint { strict }` makes them
  fail the build. Because the check runs on the DOM rather than on the
  serialized page, every finding is reported against the Typst line that wrote
  the element.

- **check**: `lint { budget { } }`, per-page weight budgets: `html`, `js`, `css`,
  `images` and `total`, each written in bytes or in the units the build summary
  prints (`"50kB"`, `1.5MB`). A page counts its own markup, the bytes of its
  inline scripts and styles, and every file it loads that this build wrote;
  responsive `srcset` candidates are excluded, since a visitor is served one of
  them. Exceeding a budget always fails the build. `baudelaire check` processes
  no assets and so runs the rules but not the budgets.

- **html**: `security { sri }`, subresource integrity from the build's own
  output. Every `<script src>` and `<link rel="stylesheet">` naming a file this
  build wrote is stamped with that file's SHA-384; a reference to another host,
  or one already carrying an `integrity`, is left alone. Needs
  `assets { fingerprint }`, and says so when it does not have it: a digest
  pinned to a name whose contents can change under it blocks the very file it
  was meant to protect.

- **html**: `security { csp { } }`, a `Content-Security-Policy` written into the
  generated `_headers`. One key per directive (`default`, `script`, `style`,
  `img`, `font`, `connect`, `frame`, `object`, `base`, `form`, `report`), each
  taking a CSP source list verbatim, plus the half no author can maintain: the
  SHA-256 of every inline `<script>`, `<style>` and `style=""` attribute the
  build produced, unioned across the site and folded into `script-src` /
  `style-src` alongside the fallback. The attributes are the ones typst resolves
  an element's CSS properties into, so a page carries several nobody wrote;
  allowing them adds `'unsafe-hashes'`, which is still an allowlist of exact
  strings this build emitted. `enforce #false` emits `Content-Security-Policy-Report-Only`
  instead. Taking those digests turns `html { pretty }` off, since the pretty
  printer re-indents an inline body after the digest is taken and a browser
  hashes what it is served.

- **typst**: `@baudelaire/pages`, the site's page catalogue as a Typst module.
  `pages(lang)` returns one row per authored page of that language, in the site's
  own order, in the same shape a listing's `entries` carry, so the card
  component a theme writes for its collection index also renders a home-page
  grid or a portfolio's work grid. Generated listings and the not-found page are
  not in it. Like `@baudelaire/sections`, it is written to a file under
  `.baudelaire/` and served from there, so only the templates that import it
  rebuild when the catalogue moves.

- **themes**: `phares`, a documentation theme: a sidebar built from your own
  `content/` tree, a search palette on `/` or `⌘K`, the page's headings down the
  right with the section being read marked, prev/next that runs the length of
  the manual, and a `callout` exported for the asides a manual needs.

- **themes**: `paysage`, a portfolio theme: a landing page with a hero, a work
  grid built from the page catalogue, and case-study pages with a cover image
  and a fact row read from each project's own frontmatter.

- **themes**: `albatros` and `spleen` gain a `home.typ` layout that lists a
  collection's newest pages from the catalogue, so a home page needs no second
  list to maintain. `spleen` gains a language switcher, still without script.

- **pdf**: A PDF of every page, beside its HTML. `generate { pdf { pages { template
  "print.typ" } } }` compiles each page a second time as a *paged* document and
  writes `/<permalink>.pdf`, with a `<link rel="alternate" type="application/pdf">`
  in the page's head pointing at it. The paged template is handed the same `page`
  dictionary your layout gets, so `page.date`, `page.reading`, `page.strings` and
  the rest mean the same thing on paper as on screen; it is a separate file
  because `html.elem` draws nothing on this target, the same split a social card
  has. Off unless the block is present, and covered by the incremental cache like
  everything else. The `slim` release does not carry the exporter, so a `pdf { }`
  block there writes nothing and links nothing.

  A paged template that wants a page rule needs `set std.page(..)`: the
  parameter named `page` by convention shadows Typst's own element.

  Every starter shape now scaffolds a `templates/print.typ`, and
  `baudelaire init --with pdf` turns it on.

- **pdf**: Many pages as one document. `generate { pdf { bundle { template
  "book.typ"; collections "guide"; site #true } } }` binds a collection end to
  end, or the whole site, into a single PDF at `/<target>.pdf` (`/guide.pdf`,
  `/site.pdf`), localized like every other per-language artifact. Pages are bound
  in the order the site already puts them, and the template is handed the
  document plus every page's `page` dictionary and compiled body, so a title
  page, a contents list and continuous numbering are yours to write. It is the
  paged counterpart of `navigation { standalone }`, which folds the same site
  into one HTML file.

  A bundle carries a cache entry of its own: it is re-exported when any page it
  binds changes, is added, removed or reordered, and when its template changes.
  The `book` and `docs` starter shapes scaffold a `templates/book.typ`.

- **html**: Source-mapped output. `html { spans #true }` stamps every element
  with the `file:line:column` the compiler says it came from, as
  `data-typst="content/post.typ:12:1"`. Typst carries a span on every node it
  emits and the typed DOM hands it through, so the location is the compiler's,
  not a guess: an inline `#emph` names its own column, and what a layout emitted
  names the template rather than the page. Elements baudelaire synthesizes (the
  meta tags, an inlined icon's innards) carry none, and neither does anything
  from a package.

  Off by default, and meant for a preview session rather than a published site.
  Turning it on changes the cache fingerprint, so the first build after is a
  cold one.

- **serve**: Alt-click the preview to open its source. `serve --spans` stamps
  the pages it serves, and alt-clicking anything on one hands its location to
  the editor named by `serve { editor "code" "--goto"
  "{file}:{line}:{column}" }`: the program and each of its arguments as their
  own word, with `{file}`, `{line}` and `{column}` filled in, run directly and
  never through a shell. The nearest stamped ancestor wins, so clicking a word
  in a paragraph opens the paragraph.

  The endpoint only answers the page it served, only opens files inside the
  project, and only exists while watching. With no `editor` configured it says
  so in the browser rather than guessing at one; every other refusal (a location
  that does not parse, a file outside the project, a command that will not run)
  arrives the same way.

- **serve**: A status dot on every served page, and a readable failure overlay.
  The dot sits in the bottom corner and stands for the live-reload connection:
  unobtrusive while it is up, marked when the stream drops or a rebuild fails,
  and clicking it brings the current state (or the last diagnostic) back.

  The diagnostic is now laid out rather than dumped: the error code as a tag,
  the message as a heading, the source frame as numbered lines with the caret
  row under them, and the `file:line:column` it names as a button that opens
  that line in your editor. The overlay no longer closes on any click either, so
  the text can be selected and copied; Escape, the backdrop, and its own button
  dismiss it.

- **docs**: A complete config reference, generated from the parser's own
  dispatch tables rather than written beside them. Every key `config.kdl`
  accepts is listed with its value shape and a description, so a key cannot be
  added without documenting it, or removed and left in the docs.

- **cli**: Every command has a short alias, listed in `--help` beside the full
  name: `b` build, `s` serve, `c` check, `n` new, `d` deploy, `a` announce,
  `i` init, `cl` clean, `comp` completions, `ref` reference.

  `check` takes `c` and `clean` takes `cl`, not the reverse: `check` is the one
  that runs in a loop while you write, and one keystroke should not separate
  compiling the site from deleting it.

- **cli**: `baudelaire reference [key]` prints the same config reference from
  the binary you have, as an indented tree. A dotted key narrows it to one
  block, which is usually what you want over a hundred and fifty keys:

  ```bash
  baudelaire reference assets.images
  ```

- **cli**: `baudelaire completions <shell>` prints a completion script for
  `bash`, `elvish`, `fish`, `nushell`, `powershell` or `zsh`, and `baudelaire
  man` prints the manual as a man page. Both write only the document, to stdout.

  ```bash
  baudelaire completions fish > ~/.config/fish/completions/baudelaire.fish
  baudelaire man > ~/.local/share/man/man1/baudelaire.1
  ```

  Both are generated from the same command definition the binary parses with, so
  they describe the build they came from: a `slim` binary compiled without
  `announce` completes and documents no `announce`.

- **cli**: `--json` reports carry a `schema` number, so a script can refuse an
  object whose shape it does not know instead of reading a field that moved. It
  is `1`, and only changes when a field changes meaning or type or goes away;
  adding a field leaves it alone.

  ```bash
  baudelaire --json build 2>/dev/null \
    | jq -e 'if .schema == 1 then .ok else error("unknown report schema") end'
  ```

### Fixed

- **build**: A sidecar file deleted from `dist` comes back. A page's HTML is
  rewritten from the cache on every build, but a social card is drawn only by
  the build that compiles its page, so clearing `dist` while keeping the cache
  left every card missing on that build and on every build after it. A page
  whose sidecar files are not on disk is now stale, which is what redraws them.

- **content**: The not-found page is no longer part of the site's own
  navigation. `content/404.typ` used to be an ordinary page everywhere but on
  disk: it took a slot in its neighbours' prev/next pager (sorting ahead of the
  home page, since ties break on source path), and published a `/404/` URL that
  nothing serves to the sitemap, the search index, `llms.txt`, the section tree,
  feeds, collection and taxonomy listings, `baudelaire:pages`, and announces. It
  still builds, to a flat `404.html`, and now has no prev/next of its own.

- **html**: A page's footnotes render inside its content instead of below the
  site footer. Typst appends the note list to the end of the document, which on
  a templated page is after everything the layout emitted, so the notes landed
  outside `<main>` and outside whatever element sets the content width, where no
  stylesheet could reach them. `html { footnotes "article" "main" }` names the
  elements they belong in, most specific first: each is tried in turn and the
  first one the page has wins, so one setting covers a post wrapped in
  `<article>` and an index that has only `<main>`. Any element a layout emits
  works, not a fixed set. Naming none leaves the notes where Typst put them, and
  so does a page that has none of the named elements.

### Upgrading

A site on `voyage` has to name another theme. `albatros` is the one that
replaced it, and the switch is a one-line config change plus copying the new
theme directory in; a site that had overridden voyage's templates keeps those
files, and they now layer over albatros, which is rarely what you want. Delete
the overrides you no longer need first.

Client code reading `baudelaire:pages` needs `p.title` renamed to `p.label`.

Footnotes moved. A page with a layout now renders its note list inside the
layout's `<article>` (or `<main>`), where it used to sit after everything else
in the body. A stylesheet that reached for it with `body > [role=doc-endnotes]`
needs a new selector; a bare `html { footnotes }` restores the old placement.

The build cache schema changed (`Renderer::SCHEMA` 8 → 10), so the first build
after upgrading is cold. Nothing to do; it is one rebuild.

`lint { strict }` and any `lint { budget { } }` can fail a build that previously
passed, which is what they are for. Neither runs on a site that declares no
`lint` block.

## [0.0.8] - 2026-07-30

### Breaking

- **permalink**: Nested content directories are part of the URL.
  `content/guide/deploy/s3.typ` publishes at `/guide/deploy/s3/`, where it used
  to flatten to `/guide/s3/`. The default permalink is now `/{path}/{slug}/`.

  This is a no-op for the conventional layout, where a collection *is* a
  directory under `content/` and `{path}` renders the same string as
  `{collection}`. It moves pages in two cases: a collection with
  subdirectories, and a collection defined by a glob over a differently-named
  directory. To keep the old URLs, set the old template explicitly:

  ```kdl
  content { collections { posts permalink="/{collection}/{slug}/" } }
  ```

  Two files with the same stem in sibling subdirectories used to be a hard
  output collision; they now coexist.

- **cli**: Every boolean flag is a `--x` / `--no-x` pair. `--strict-links=false`,
  `--open=false` and `--draft=false` no longer parse; write `--no-strict-links`,
  `--no-open`, `--no-draft`. `--cache` and `--watch` are new, as the positive
  halves of `--no-cache` and `--no-watch`.

  The point is reversibility: a setting turned on in `config.kdl` had no CLI
  route back off, so `draft { build #true }` could not be overridden for one
  production build. Now `--no-drafts` does it. Passing both halves, the last one
  wins.

- **cli**: One name per concept. `announce --password` is `announce --secret`,
  the same spelling `deploy` uses for the same parameter, and `new --open` is
  `new --edit`, since `serve --open` opens a browser and the short form was
  already `-e`. Both old spellings stay as aliases.

- **config**: A collection is a `{ }` block, and its generated index nests
  inside it. `paginate`, `list`, `mount` and `prefix` were four flat attributes
  sitting beside the ones that shape *member* pages, and three of the four names
  did not say what they meant: `list` was a template rather than a list, `mount`
  the permalink of page 1, `prefix` the segment before a page number. Reading
  `template` next to `list` gave no clue that the first wrapped a post and the
  second the index over them.

  ```kdl
  // before
  content { collections { blog sort="date" template="post.typ" list="index.typ" paginate=5 } }

  // after
  content {
    collections {
      blog {
        sort "date"
        template "post.typ"
        paginate { template "index.typ"; size 5 }
      }
    }
  }
  ```

  The `paginate { }` block's presence is what generates the index, as
  `robots { }` and `spa { }` work; a block with no `size` puts every member on
  one page, which is what a `list` without a `paginate` was. The glob keeps its
  positional shorthand (`blog "blog/**/*.typ" { .. }`).

- **config**: `generate { search { client } }` is `generate { search { ui } }`.
  It turns on the shipped Ctrl-K palette, and the top-level `client { }` block
  is build-time constants for client JS: one word meant two things, and both of
  them were literally about client-side JavaScript, so the enclosing block did
  not disambiguate. Rename the key; nothing else changes.

- **config**: `assets { images { optimize { jpg } } }` is spelled `jpeg`. It was
  a second key onto the same field, so a block naming both configured one format
  twice with the last winning and no duplicate diagnostic, while the "valid
  keys" help offered them as if they were different formats. File extensions
  stay lenient: `.jpg`, `.jpeg`, `.jpe` and `.jfif` all match the `jpeg` block.

- **clean**: The wholesale sweep asks before it removes anything, and refuses
  to answer for itself off a terminal. `clean` with no flag takes the output
  directory and every scrap of local state, announce state included, which is
  what the next `announce` reconciles a live repository against; the only guard
  was a check that the path did not contain the project. Pass `--yes` in CI, or
  `--dry-run` to see the list. A narrowed `clean --cache` still runs unasked.

- **init**: `-y` no longer sets up version control. It means one thing now,
  "take the default answer to every prompt", so the flag a script reaches for to
  silence the prompts stops leaving a git repository behind. Naming `--vcs` is
  how you ask for one: `baudelaire init -y --vcs git` restores the old
  behaviour.

- **announce**: Announcing is behind a default-on `announce` cargo feature. No
  change to a normal build; a `--no-default-features` (slim) binary loses the
  `announce` command and stops emitting the standard.site verification
  artifacts, warning when a config asks for them.

- **cache**: `page.sections` is gone. Templates import `sections(page.lang)`
  from `@baudelaire/sections:0.1.0`. The tree named every page on the site from
  inside each page's cache fingerprint, so one retitle was a cold rebuild of
  everything; as a module, typst's own dependency tracking scopes it to the
  templates that render a nav. A blog of 30 posts now reuses 38 of 42 pages on a
  retitle, where both were full rebuilds.

### Added

- **check**: Link fragments are validated against the target page's headings.
  `#link("/content/guide.typ#setup")` must find that heading, so renaming one
  reports every deep link into it instead of quietly breaking them. Runs
  site-wide on every build, including over cached pages, so a rename in one page
  is caught without the page linking to it having changed.
- **feed**: Feed items carry a description, categories, and both dates. An entry
  takes `description` (or `summary`) and its taxonomy terms from frontmatter;
  previously an item was a bare title, which is what a reader showed. Atom gains
  `published`/`summary`/`category`, JSON Feed `summary`/`date_modified`/`tags`,
  RSS `description`/`category`.
- **feed**: Every page advertises the configured feeds in its `<head>`. Feeds
  were emitted and nothing pointed at them, and since typst-html owns `<head>` a
  layout could not add the tag.
- **generate**: `generate { redirects }` writes a `_redirects` rule file that
  Netlify and Cloudflare Pages answer with a real 301, instead of the per-path
  HTML stubs. It replaces them rather than joining them: both hosts serve a
  static file in preference to a redirect rule, so a stub left at the old path
  would win and the 301 would never fire. Off by default, since a stub works on
  any host.
- **taxonomies**: Term pages paginate. `tags listing=#true paginate=20` chunks
  `/tags/rust/` across `/tags/rust/page/2/` and so on, with the prev/next nav
  every other listing carries, and `prefix=` renames the `page` segment. A term
  held every page under it whatever its size, beside a collection index that
  paginated the very same pages; both now run through one rule, so they cannot
  disagree about what page 2 is called.
- **html**: A dated page carries the `article:*` vocabulary
  (`published_time`, `modified_time`, `author`, and one `tag` per taxonomy
  term), and a social image carries `og:image:alt` from an `alt` frontmatter
  key. Several unfurlers read `article:published_time` for the dateline. A
  generated card describes itself: it draws the page title.
- **html**: `html { jsonld }` emits a schema.org JSON-LD island per page, an
  `Article` where the page is dated and a `WebPage` otherwise. Built from the
  same facts as the meta tags, so the two cannot claim different things about
  one page. Off by default, unlike its neighbours: those restate what the page
  already says, while structured data is a claim made to a search engine.
- **i18n**: Dates are written the way their language writes them. Every page
  carries `page.date.iso` and `page.date.display`, and every listing entry
  `entry.date` and `entry.display`: the first is the ISO-8601 day a
  `<time datetime>` or a feed wants, the second what a reader wants. A language
  declares its own through two `strings` keys, `date` (a pattern over `{day}`,
  `{day2}`, `{month}`, `{year}`) and `months` (the twelve names), so there is no
  locale database in the binary. Every listing showed ISO-8601, and typst could
  not fix it in a template: its own `datetime.display` knows English months
  only.
- **config**: A `strings { }` or `client { }` entry with several arguments is a
  list. Only the first was read, so `months "janvier" "février" ..` kept January
  and dropped the rest.
- **content**: `page.reading` reaches every template: `reading.words` and
  `reading.minutes`, for the "6 min read" line a blog index wants. Counted from
  the page's typst source rather than its rendered HTML, because the source is
  the only version in hand when a template is handed its page, so code lines
  (`#import`, `#let`) are skipped as machinery and the figure is an estimate.
- **content**: `translation` frontmatter, so a translated page can take a
  translated slug. Editions pair on collection and slug, so a French edition had
  to keep the English one: rename it and it became a standalone page, losing the
  switcher and its `hreflang` alternates. Naming the same `translation` key on
  both pairs them outright, and `/posts/hello/` and `/fr/posts/bonjour/` link to
  each other. The key never appears in a URL.
- **content**: `expiry` frontmatter, the last day a page is published. From the
  day after it leaves the build entirely (no page, no listing entry, no feed
  item, and `prune` removes what an earlier build wrote), which is the other end
  of the window `content { future }` opens: an event that has happened, a call
  for papers that has closed. Nothing brings it back, so it is a decision rather
  than a preview; a page still being written is a `draft`.
- **content**: `updated` frontmatter, for when a page last changed. `date` stays
  the publication date and still orders every listing, so a rewritten 2023 post
  is still a 2023 post; `updated` is what the sitemap's `lastmod` and a feed
  entry's `updated` report. Both previously read `date`, so a rewrite told
  crawlers nothing had changed.
- **html**: `html { highlight { } }` rewrites syntax-highlight colours as CSS
  classes. typst bakes them inline with no class option, so a dark-mode site had
  to author a `.tmTheme` of sentinel hex values and reclaim them with
  `pre code [style*="e5d004"] { ... !important }`. Naming a scope
  (`keyword "#e5d004"`) emits `class="sx-keyword"`; unnamed colours fall back to
  `sx-<hex>`.
- **cli**: `--version` reports the build, not just the number: the commit it
  was built from (with a `-dirty` suffix when the tree had uncommitted
  changes), the rustc and profile, the target, and the optional features
  compiled in. A binary missing any gains a `without` row naming them, which is
  the answer to "why is my `assets { bundle }` doing nothing": five cargo
  features gate whole modules and the released `slim` flavor turns all of them
  off, and until now nothing in the CLI reported which flavor you were holding.
  `-V` stays the one line a script greps.
- **cli**: `--strict` fails a run that warned. The warning tally existed but
  only `--strict-links` could gate on it, one class out of the whole set.
- **clean**: `--all`, `--dry-run` and `--output`. `--all` is the sweep-everything
  mode said out loud, so the most destructive invocation stops being the
  shortest one by accident; `--dry-run` prints the directories and removes
  nothing; `--output` names what it removes rather than the config key that
  locates it (`--dist` remains as an alias). The paths are now printed before
  the sweep, not only under `-v`.
- **cli**: `deploy` and `announce` take the build flags. Both build the site
  before publishing it, so `--base-url`, `--drafts`, `--future`, `--out` and
  `--no-cache` now shape that build; a named profile used to be the only lever,
  which forced every preview permutation into `config.kdl`.
- **cli**: `--json` writes a machine-readable summary of the run to stdout:
  `ok`, `pages`, `cached`, `warnings`, and every diagnostic with its code and
  severity. stdout was reserved for data and had never carried any.
- **serve**: A failed rebuild is overlaid in the browser. It reached the
  terminal only, so a tab kept showing the last good page with no way to tell a
  broken save from a slow one.
- **caching**: A top-level `caching { }` block sets the `Cache-Control` the
  built files are served with. Content-addressed files get
  `max-age=31536000, immutable`, everything else revalidates; the split is
  derived from `assets { fingerprint }`, which is what makes hashing a filename
  worth anything at the last step. A `deploy` sets it per object on S3, and
  `generate { headers }` writes the same policy into a `_headers` file for
  Netlify and Cloudflare Pages, so a site doing both cannot state two different
  answers. (Not to be confused with `cache { }`, the build cache.)
- **generate**: `generate { headers }` writes `_headers` from the `caching`
  policy. Needs both: a rule file with no policy in it says nothing the host had
  not already assumed.
- **deploy**: The SSH backend is behind a default-on `ssh` cargo feature. It is
  the most expensive thing in the tree after typst and rolldown by crate count.
- **themes**: Three themes ship in `themes/`: `albatros` (a centred blog),
  `spleen` (a terminal, no JavaScript), and `voyage` (a multilingual journal
  with a language switcher). Each is a complete look, templates and assets and
  config defaults, and each is overridable file by file. None hardcodes a menu:
  the nav is derived from `@baudelaire/sections`, so it follows `content/`.
- **typst**: `typst { registry }` names a mirror of Typst Universe to download
  the `preview` namespace from, for a build behind a proxy or on a machine that
  cannot reach `packages.typst.org`. It covers a page's own `#import` and the
  site's `theme` alike, since both resolve through one package store; a plaintext
  URL is refused, as package tarballs are code the build runs. Every other
  namespace is served from the local package directories exactly as before, so a
  mirror never changes where an already-installed package comes from.

### Fixed

- **prune**: A `dist` containing the sources no longer deletes them.
  `paths { dist "." }` took `config.kdl`, the content tree and every unrelated
  file in the project, and reported a successful build. A `dist` that is the
  project root, or an ancestor of any other `paths` entry, is now refused.
- **search**: The build-time and query-time tokenizers agree. They disagreed in
  two ways: the index stripped punctuation before lowercasing, so `İstanbul` was
  keyed `i̇stanbul` while a query asked for `istanbul`; and the client retained
  `\p{L}\p{N}` where the index retained Unicode `Alphabetic`, dropping the Indic,
  Arabic and Hebrew vowel marks the index kept. Either made a page unfindable.
- **deploy**, **announce**: A run with no terminal and no `--yes` fails instead
  of silently doing nothing. The confirmation prompt returned its default, which
  is "no", so a CI job that forgot `--yes` skipped every destination and exited
  0.
- **config**: A bare `generate { robots }` turns the feature on, as documented.
  It was a hard `missing_children` error, so the spelling the docs prescribed
  did not parse. A section that only holds settings still requires its block.
- **config**: `content { index }` rejects a filename. It names a stem, so the
  documented `index "index.typ"` matched no page: the site built green with
  nothing at `/`.
- **deploy**: An unstated `deploy { s3 { region } }` follows the target. It
  defaulted to `us-east-1` whatever the `endpoint` said, so an R2 bucket was
  signed under an AWS region and answered 403 with nothing in it naming the
  cause; a custom endpoint now signs as `auto`. A stated region is unchanged.
- **config**: A collection's `glob` can be written as `glob="..."`, not only as
  the leading positional. It is the field's name in the docs and in the struct,
  and writing it failed with a help that listed every key except that one.
- **init**: `--config` and `--profile` stop being accepted and ignored.
  `--config` now names the config file to scaffold, so
  `init --config site.kdl` writes one every later command finds under the same
  flag; a path rather than a bare filename is refused, since `paths { }`
  resolves against the working directory and a nested config would name a
  content tree outside its own project. `--profile` is refused outright.
  Separately, `--vcs` stops claiming to imply `--yes`: it skipped only the
  version-control prompt, so a scripted `init --vcs git` still blocked on
  "Author".
- **mime**: `.jfif` and `.jpe` are served as JPEG, and extensions match
  regardless of case. The optimizer and the MIME table kept separate lists, so a
  file optimized as a JPEG was served as `application/octet-stream`; `Photo.PNG`
  hit the same split.
- **cache**: Two render-side inputs that served stale output.
- **clean**, **new**: Neither refuses over a config it does not need. `clean` is
  what you reach for when the project is in a state you want gone, and a config
  syntax error blocked it; it now warns and sweeps the built-in directories
  instead. A config that is *missing* is still an error, since sweeping `public`
  and `.baudelaire` out of whatever directory you were standing in is not a
  recovery. `new` writes the page when the project cannot be opened, losing only
  the two conveniences that read existing content: the next `order` and the
  permalink-collision check.
- **config**: A setting that does nothing because a sibling is off now says so.
  Five of them were accepted, changed nothing, and reported nothing:
  `assets { minify }` leaves JavaScript verbatim without `bundle`;
  `generate { feed { terms } }` writes no file unless a taxonomy has `listing`;
  `generate { search { stopwords } }` and `{ minimum }` tune only the `inverted`
  index; `announce { standard { verify } }` emits nothing without a `did`, and
  defaults on. Each warns once per build, naming what it needs.
- **serve**: `--port 0` prints the port it got. It means "any free port", and
  the banner answered by advertising `http://127.0.0.1:0/`.
- **cli**: `-v` wins over `RUST_LOG`. Any value in the environment used to
  discard the verbosity count, so `RUST_LOG=warn baudelaire -vv build` printed
  no debug events and said nothing about why. A run that passes no `-v` still
  honours the variable, which stays the only way to see a dependency's events.

- **links**: `links { orphans "any" }` reports the pages nothing links to;
  `orphans "authored"` reports the pages nobody *wrote* about.

  ```
  ⚠ 2 pages linked from nowhere
    ⚠ `guide/exporting.typ` is linked from nowhere, and serves at `/guide/exporting/`
  ```

  A link counts when an author wrote it, spelled as a `.typ` path or as a URL. A
  layout never does: a sidebar links every page from every page. The mode decides
  whether the build's own listings count: under `any` a paginated index and a
  term page are ways in, so the report names only pages a reader cannot get to;
  under `authored` they are not, which names a post reached from its index and
  from nowhere else. The root of each language, the listings themselves and the
  not-found page are left out of both.

  A listing's entries are read from the page set, not from its markup, so a
  listing with a template of its own counts like the default one.

  A report, never a failure. Either switch turns the link graph on, so a site
  that wants only the report pays for the edges and none of the second compiles.

### Performance

- **cache**: Link dependencies are tracked per page, so a permalink change
  rebuilds the pages that link to it rather than the whole site.

### Upgrading

The build cache schema changed (`Renderer::SCHEMA` 7 → 8), so the first build
after upgrading is cold. Nothing to do; it is one rebuild.

Anchor checking can fail a build that previously passed, since `links { strict }`
defaults on and a dangling `#fragment` was never looked at before. Run
`baudelaire check` before upgrading in CI, or set `links { strict #false }` to
take these as warnings.

## [0.0.7] - 2026-07-28

### Breaking
- Regroup the config tree by concern
- Rename the colliding index config keys

### Added
- SPA navigation and single-file HTML export
- Link checking, social cards, themes, virtual Typst modules
- Inline SVG icons with `svg()`, confined to the icon
- **init**: Four starter templates behind a registry

### Fixed
- **install**: Tolerate whitespace in the tag_name JSON
- **search**: Drop empty segments when joining generated URLs

## [0.0.6] - 2026-07-26

### Added
- Externalize typst's embedded images

### Fixed
- **ci**: Bump actions pin for sccache fix

## [0.0.5] - 2026-07-26

### Added
- **i18n**: Multi-language sites via `.lang.typ` suffix
- Always enable the typst `html` feature; docs sync
- Allow disabling typst features with `-name` (except `html`)
- **docs**: Copy buttons on code blocks

## [0.0.4] - 2026-07-17

### Added
- **install**: Fetch musl binaries on musl systems
- **graph**: Track `sys.inputs` reads per value for incremental builds
- **deploy**: S3-compatible file deploy
- **deploy**: SSH/SFTP backend
- **deploy**: SSH host-key pinning and agent auth
- **deploy**: Clear diagnostic for a changed ssh host key

### Fixed
- **tests**: Rebase assets/templates/static in `Site::config`

## [0.0.3] - 2026-07-17

### Added
- Support subpath hosting

## [0.0.2] - 2026-07-17

### Added
- `js`/`css` feature gates
- Build the slim preset in release

### Fixed
- Make `--dry-run` in atproto publishing unauthenticated

## [0.0.1] - 2026-07-17

### Added
- Incremental content cache
- Assets/images optimization, srcset/poster/CSS `url()` fingerprint rewriting
- Atproto standard.site announcing
- JSON feed, site 404 page, dark-mode favicon
- Template navigation data (`page.nav` + `page.sections`)
- Un-paginated listings and configurable pagination prefix
- Taxonomies, feed and virtual JS modules; `client` exposed to templates
- Nested sections
- Styled, grouped CLI help; content structure inferred in `new`
- **init**: Git init in scaffolding; site name resolved to its own directory

### Fixed
- Strict config parsing, precise errors, nothing swallowed silently
- Typed frontmatter errors, config-driven taxonomy keys, per-span eval labels
- Cache correctness: link/embed fingerprints, atomic verified blobs
- Output-file collisions, ASCII slugs, empty pagination, rooted links
- Profile overlay preserves sibling fields in nested sections
- Reject path traversal in the dev server file resolver
- Embed inlines processed asset bytes instead of raw source
- Warn on an unreadable cache manifest instead of rebuilding silently
- Reap disconnected SSE streams via heartbeat and self-removal
- Config reloads in `serve`
- Stale skip-cache and empty stdin secret on publish
- CSS import order, `url()` tails, EXIF rotation in assets
- Orphans properly cleaned by `clean`

[Unreleased]: https://github.com/cestef/baudelaire/compare/v0.0.10...HEAD
[0.0.10]: https://github.com/cestef/baudelaire/compare/v0.0.9...v0.0.10
[0.0.9]: https://github.com/cestef/baudelaire/compare/v0.0.8...v0.0.9
[0.0.8]: https://github.com/cestef/baudelaire/compare/v0.0.7...v0.0.8
[0.0.7]: https://github.com/cestef/baudelaire/compare/v0.0.6...v0.0.7
[0.0.6]: https://github.com/cestef/baudelaire/compare/v0.0.5...v0.0.6
[0.0.5]: https://github.com/cestef/baudelaire/compare/v0.0.4...v0.0.5
[0.0.4]: https://github.com/cestef/baudelaire/compare/v0.0.3...v0.0.4
[0.0.3]: https://github.com/cestef/baudelaire/compare/v0.0.2...v0.0.3
[0.0.2]: https://github.com/cestef/baudelaire/compare/v0.0.1...v0.0.2
[0.0.1]: https://github.com/cestef/baudelaire/releases/tag/v0.0.1
