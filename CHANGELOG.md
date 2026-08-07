# Changelog

Notable changes to baudelaire. Format follows [Keep a Changelog][kac]; versions
follow [Semantic Versioning][semver], with the pre-1.0 caveat that a breaking
change bumps the patch number.

Nothing that only affects the repository is listed: refactors, tests, CI and
chores are visible in the git history and change nothing for a site.

[kac]: https://keepachangelog.com/en/1.1.0/
[semver]: https://semver.org/spec/v2.0.0.html

## [Unreleased]

### Upgrading

- **A theme's `theme.kdl` may no longer carry a `generate { headers { } }` rule
  or a wildcard `redirect`**, and a build using such a theme now fails where it
  previously took them. Both are new in this release, so nothing that worked
  before stops working; the refusal is stated here because it is a build that can
  go red on a theme you did not write.

  A theme is *fetched*, so its defaults are text the site never read. A header
  rule is an arbitrary response header on an arbitrary path (`Refresh` forwards
  every page elsewhere; `Access-Control-Allow-Origin` hands the site's content to
  any origin), and a wildcard redirect claims no output file, so the collision
  check that stops a theme's redirect burying a real page has nothing to compare
  it against. A theme may still turn both rule files on and declare literal
  redirects: what goes in `_headers` is then computed from the site's own
  `caching` and `csp`.

- **`generate { search { region } }` and its `ignore` moved to
  `html { region { element } }`** and `html { region { ignore } }`. The old
  spelling is an unknown key and fails the build.

  ```kdl
  html {
    region {
      element "article"
      ignore "nav" "aside"
    }
  }
  ```

  Which element holds a page's prose is a fact about the markup a site emits, not
  about the search index: a full-content feed asks the same question, and under
  the old spelling it would have had to read the search block or ask again under
  a second name.

- **`description`, `summary`, `image`, `alt` and `author` are recognized
  frontmatter keys** rather than conventions read out of a page's extra
  frontmatter by name. Three consequences, all of which can fail a build that
  previously passed:

  - A wrong-typed value is now an error naming the file and the field. It used to
    read as absent, so a page shipped without a description out of a green build.
  - A near-miss (`descripton`, `athor`) is reported as a typo instead of passing
    silently into `extra`.
  - A collection schema may declare them, and one declaring a different type
    (`author "dict"`) is refused at the config line. Name a key of your own
    (`editor`, `authors`) for a structured value.

  They also leave `extra`. A template reading `page.frontmatter.description` is
  unaffected (a `.typ` page's dict is its own binding), but a listing row that
  read `entry.extra.summary` now reads `entry.description`, and
  `entry.image`, `entry.alt` and `entry.author` join it on every row and in
  `baudelaire:pages`.

  A key that merely *extends* one of them is not a typo: `authors` and `images`
  pass through as they always did. So does any key a collection's schema
  declares.

- **A `redirect` line is now read as a line, not as a bare pair.** It gained a
  `status` attribute, and with it the checks every other line in the config
  already had. Four spellings that used to be accepted (three of them silently
  doing nothing) now fail:

  - two lines claiming the same old path, which used to have the second
    overwrite the first at emit time
  - a third positional, `"/old/" "/new/" "/extra/"`
  - any attribute other than `status`
  - a `{ }` block on the line

  The documented spelling, `"/old/" "/new/"`, is untouched.

- **A site that ships its own fonts rebuilds when one of them changes.** The
  faces under `typst { fonts { paths } }` are now part of the build fingerprint,
  so the first build after this upgrade is a cold one for those sites. Every
  other site is unaffected: nothing is walked or hashed when no directory is
  named.

### Added

- **A redirect names the status it forwards with:**

  ```kdl
  redirect {
    "/moved/" "/new/"
    "/beta/" "/preview/" status=302
  }
  ```

  `301` remains the default, and every pair a site already wrote keeps its
  spelling: the target is still the line's one positional and `status` is an
  attribute beside it. Anything outside `300`-`399` is refused. The status only
  reaches a host through `generate { redirects }`, and setting one without it is
  reported as inert rather than silently written as a 301. Reading the line as a
  line also brings the checks every other one has; see **Upgrading**.

- **A lint rule carries its own severity**, so `strict` is a default rather than
  an override:

  ```kdl
  lint {
    strict
    headings "warn"
    ids "off"
  }
  ```

  `off`, `warn`, `error`. A rule that names none follows `strict`, which is the
  whole of the old behaviour, and the boolean spelling still works: `#true` is on
  at the default, `#false` is off. Until now a rule was on or off, so exempting
  one from `strict` meant losing it.

- **A budget can report instead of failing**, with
  `lint { budget { strict #false } }`. A budget is an assertion the author wrote
  down and still fails by default; a site adopting one on pages it already has
  needs a number to aim at first.

- **`html { meta }` is a block**, holding the two social facts a page cannot
  state for itself:

  ```kdl
  html {
    meta {
      twitter "@example"
      image "/og.png"
    }
  }
  ```

  `twitter` fills `twitter:site`, which credits the site rather than whoever
  posted the link. `image` is the preview for a page that names none and gets no
  generated card: a floor, so a page's own `image` and a generated card both win.
  `html { meta #false }` means what it did.

- **A feed can carry each entry in full**, not just its one-line summary:

  ```kdl
  generate {
    feed {
      formats "rss" "atom"
      content "full"
    }
  }
  ```

  The body comes from `html { region }`, the same part of the page the search
  index reads, so neither the chrome around it nor anything `region { ignore }`
  names inside it travels. A layout that emits no region falls back to `<body>`,
  never to the whole document. The summary is kept beside it, in the element each
  format has for one: `content:encoded` for RSS, `<content type="html">` for
  Atom, `content_html` for JSON Feed.

- **A page opts out of the files a build generates about it**, with an `exclude`
  list in its frontmatter:

  ```typ
  #let frontmatter = (
    title: "Thanks for subscribing",
    exclude: ("sitemap", "search"),
  )
  ```

  `sitemap`, `feed`, `search`, `card`, `pdf`. Each was all-or-nothing per site: a
  thank-you page was in the sitemap, a changelog was in the feed, and a page
  could say nothing about either. A name outside the list fails the build.

  It names generated *files*, not listings: a page left out of the search index
  is still listed by its collection index. `feed` covers every feed a build
  writes, the per-term ones included.

- **`html { anchors }` is a block, and can emit the self link it always claimed
  to.**

  ```kdl
  html {
    anchors {
      levels 2 3 4
      link "#"
      place "after"
    }
  }
  ```

  The key documented itself as giving a heading "an `id` and a self link" and only
  ever gave it the id. `link` is opt-in, because it is markup the site did not
  write; when on, it carries `class="anchor"`, `aria-hidden` and `tabindex="-1"`.
  `levels` narrows which headings are covered at all. `html { anchors #false }`
  means what it did.

- **A taxonomy orders its term listings**, with the keys a collection already
  has:

  ```kdl
  content {
    taxonomies {
      tags listing=#true sort="date" reverse=#true
    }
  }
  ```

  A term page sorted by title unconditionally while the collection index beside
  it honoured `sort`, so the same posts came in two orders on one site. Both now
  read one comparator. The default stays `title`, since a term spans collections
  and `order` is a number each collection assigns for itself.

- **The reading rate is configurable, and a language may state its own.**

  ```kdl
  content {
    reading { wpm 250 }
  }
  languages {
    ja { name "日本語"; wpm 600 }
  }
  ```

  `page.reading.minutes` was 200 words a minute, a constant. The figure is a fact
  about the language: Japanese and Chinese are read several times faster by word,
  so a site in one reported every article as a fraction of the read it is. A
  language with no `wpm` falls back to `content { reading { wpm } }`, the same
  way `author` and `description` do.

- **`assets { targets { } }`**: the oldest browsers the CSS must run on.

  ```kdl
  assets {
    targets {
      chrome "100"
      safari "15.4"
    }
  }
  ```

  Naming any turns Lightning CSS's transform on: nesting is flattened, vendor
  prefixes are added, and modern colour syntaxes get fallbacks, each only where a
  named browser needs it. Until now only its minifier ran, so a site writing CSS
  nesting shipped it unflattened to browsers that cannot read it.

  Independent of `minify`: a site can compile its CSS down and still ship it
  readable. A version is one to three numbers, each 0-255, written as a string
  (`15.10` as a KDL number is the float `15.1`).

- **`assets { minify }` is a block**, so the two kinds are separate asks:

  ```kdl
  assets {
    minify {
      js #false
    }
  }
  ```

  `css` and `js`. The block's presence turns both on and `minify #false` turns
  both off, so `minify #true` means what it always did.

- **`typst { fonts { } }`**: the faces a build can see.

  ```kdl
  typst {
    fonts {
      paths "assets/fonts"
      system #false
    }
  }
  ```

  `paths` names directories scanned recursively, searched after typst's bundled
  faces and before the machine's, so a face a site ships wins over a same-named
  installed one. `system #false` drops the machine's entirely, which is what
  makes a build reproducible: until now the output depended on what happened to
  be installed, and a site had no way to ship a face at all.

  A directory that is not there fails the build. Scanning one yields no faces and
  no error, so the typo would otherwise surface as a page silently typeset in a
  fallback.

- **`links { external }` is a block**, so the outbound check can be tuned instead
  of only turned off:

  ```kdl
  links {
    external {
      fresh "7d"
      timeout "10s"
      concurrency 4
      ignore "*.internal/**"
      accept 401 429
    }
  }
  ```

  `fresh` is how long a link that answered is trusted, `timeout` how long one
  request may take, `concurrency` how many are in flight at once (a limit on
  these requests alone, not on the rest of the build). `ignore` drops URLs before
  they are requested, in the glob grammar `prune { keep }` uses, matched against
  the URL without its scheme so one pattern covers `http` and `https`. `accept`
  names status codes that count as alive beyond 2xx and 3xx: a page behind a
  login answers 401 and is still there.

  A duration is a number and an optional unit (`250ms`, `30s`, `5m`, `2h`, `7d`);
  a bare number is seconds. `links { external }` and `links { external #false }`
  mean exactly what they did.

- **A page's body can be a file the site does not publish**: a `CHANGELOG.md` at
  the top of the repository, a `README.md` shared with another project. The
  config declares the file under a name, and a markdown page names the name:

  ```kdl
  paths {
    sources {
      changelog "../CHANGELOG.md"
    }
  }
  ```

  ```md
  ;;;
  title "Changelog"
  source "changelog"
  ;;;
  ```

  A page names a *name* and never a path, and `paths { sources }` is the only
  place a path is written, so a page cannot reach a file the config did not offer
  it. A theme cannot add one either: `paths` is among the sections a theme is
  refused. The declared path may sit outside the project, which is the case this
  exists for.

  The file is read as markdown under its own name, so a fault in it is reported
  where the prose is. A `source` beside a body of its own is an error, and so is
  one on a `.typ` page, which has `#include` and typst's own dependency tracking.

- **A `redirect` old path may carry `*`**, matching a family of URLs rather than
  one:

  ```kdl
  redirect {
    "/latest/*" "/:splat"
  }
  ```

  What the destination says back is the host's own grammar and is passed through
  untouched. A pattern needs `generate { redirects }`: an HTML stub is a file at
  one path, and a family of URLs has no single path to put one at. Without it the
  pattern is dropped and the build says so, rather than writing a stub into a
  directory literally named `*`.

- **`generate { headers { } }`** now takes rules of the site's own, beside the
  `Cache-Control` and CSP it already derived:

  ```kdl
  generate {
    headers {
      "/private/*" {
        X-Robots-Tag "noindex"
      }
    }
  }
  ```

  A path pattern, and the headers it sends. These lead the file, ahead of the
  derived rules and the catch-all that ends it, and a pattern is prefixed with
  the site's base path like every other pattern in it. `generate { headers }`
  keeps both spellings it had: the bare flag, and `#false` in front of a block.

- **`prune { keep }`**: globs, relative to the output directory, the sweep never
  deletes:

  ```kdl
  prune {
    keep "themes/**" "v*/**"
  }
  ```

  For a `dist` that receives more than this build: a second site published under
  a subdirectory, artifacts another tool writes there. Until now the only way to
  combine the two was to run the pruning build first, an order nothing enforced
  and no error reported.

  `prune` keeps every spelling it had. `prune #false` turns the sweep off,
  `prune` and `prune #true` turn it on, and the flag now stands in front of the
  block: `prune #false { keep .. }` parses and stays off.

- **`--theme <theme>`**: build with a theme the config does not name, or with a
  different one:

  ```sh
  baudelaire build --theme themes/albatros
  baudelaire build --theme @preview/plume:1.0.0
  ```

  Global, beside `--profile`, and takes what `theme` in the config takes: a
  directory inside the project, or a package. A profile still cannot name a
  theme, and this is the reason it exists: the theme's `theme.kdl` supplies the
  floor the site's own keys are layered over, so it has to be known while the
  config is being read, and a profile is overlaid after that.

- **`assets { sourcemap }`**: source maps, so a minified bundle reads in
  devtools as the files it was built from. One word says what becomes of the
  map, per kind of asset:

  ```kdl
  assets {
    sourcemap "external"            // both kinds

    sourcemap "external" {          // ..or set both and narrow one
      styles "off"
    }

    sourcemap {                     // ..or name each
      scripts "hidden"
      styles  "inline"
    }
  }
  ```

  `off` writes nothing; `inline` puts the map inside the file as a `data:` URI;
  `external` writes it beside the file and names it in a `sourceMappingURL`
  comment; `hidden` writes it and points at it from nowhere, which is the shape
  for uploading to an error tracker.

  Off by default, and it has to be opt-in: the pipeline never publishes the
  `.ts`, `.jsx` or unminified `.css` a map would name, so each map carries the
  original text inside it, and asking for one publishes your sources.

  The link survives `fingerprint`: the served name is hashed before the
  `sourceMappingURL` comment is appended, so the comment cannot change the name
  it points at, and the map is named after the file it maps, hash included.

## [0.0.12] - 2026-08-06

### Upgrading

- `content { draft { .. } }` is now `content { drafts { .. } }`, and the key that
  matters takes a bare flag:

  ```kdl
  content { draft { build #true } }   // was
  content { drafts #true }            // is, and `drafts { build #true }` still reads
  ```

  The old spelling is an unknown key, not a silent no-op: a `draft` block that
  parsed and configured nothing is a production build quietly dropping every
  draft page. `drafts` with no value at all also enables them, like any other
  flag; `drafts #true { suffix ".wip" }` sets both.

- Every `profiles { .. }` entry is now checked when `config.kdl` is read, so a
  config that only ever built green because nobody passed `--profile` to the
  profile holding the typo will go red. The error is the one selecting it would
  have raised, at the same span.

- **A `.md` file under `content/` is now a page.** It used to be ignored, so a
  `README.md` or a note left beside your pages was invisible; it now builds, and
  a build that passed can go red -- on frontmatter it never had to declare, on
  a `---` block that is not valid YAML, or on raw HTML, which a README often
  carries. To keep such a file where it is and publish nothing for it:

  ```kdl
  content { markdown #false }
  ```

  Prefixing the file with a dot works too (dotfiles are still skipped), as does
  building without the `markdown` feature.

- **A value written on a block's own line is now read or refused, never
  dropped.** A node-keyed line was dispatched on its name alone, so anything
  else on it was accepted and discarded. Every one of these parsed green and
  configured nothing:

  ```kdl
  lint #false                          // turned linting ON
  html { highlight #false }            // turned highlighting ON
  generate { cards #false }            // enabled cards
  serve { port 1 2 }                   // dropped the 2
  content { drafts suffix=".x" }       // kept `.draft`
  content { collections { posts sort="date" } }  // read `date` as the glob
  ```

  Each now does what it says or errors naming the key, and where the pair
  belongs one level in the help writes the line to use (`drafts { suffix ".x" }`).
  The first three read as the off switch described below; the rest are errors.
  Nothing that was ever read has changed meaning, so a config that meant what it
  said needs no edit; a config that did not will say so on the next build.

  `html { highlight #false }` is the one worth checking by hand: it was the
  spelling for turning highlighting off, it did the opposite, and there was no
  other spelling, so a site that wanted it off has been shipping it on. Turning
  it off now works and the rendered HTML changes accordingly.

- **A theme's `theme.kdl` can no longer set what the site's machine does.**
  `paths`, `hooks`, `announce`, `deploy` and `profiles` are refused outright,
  where they used to be inherited by any site that did not state its own. A
  theme carrying one now fails the build that adopts it, naming the section.

  `hooks` is why: it runs commands through the system shell, so an inherited
  `hooks { before }` ran code on every build of every site using that theme, and
  for a `@preview/` theme fetched at build time that code appeared nowhere in
  the project. Move the block into the site's own `config.kdl`; a theme needing
  a build step should say so in its README and give the block to copy.

- **A `#fragment` link into a page's own body is now checked.** It used to pass
  through unexamined, so `#link("#install")` naming no heading on the page was
  published as a dead anchor out of a green build. It is now resolved like any
  other deep link: a warning normally, and fatal under `links { strict }`, which
  is on by default. A skip-link or a JS-driven toggle whose target is not a real
  id is the usual first offender.

- **A mistyped `paths { content }` fails instead of building nothing.** A
  content directory that does not exist used to yield zero pages and exit 0, and
  with `prune #true` that swept the published site away. A path the site *named*
  is now an error; the default `content` staying absent is still quiet, since a
  fresh scaffold and a site of nothing but `static` files both look like that.
  A build that produces no pages at all now also leaves `dist` alone whatever
  `prune` says, and says why.

- `Renderer::SCHEMA` moved 16 to 17, so the first build after upgrading is a
  cold one. Heading ids are slugged in document order, a `srcset` candidate no
  longer repeats the directory its image was authored under, every vocabulary
  names the same author, and a page records the fragments into its own body: an
  entry written under the old rules holds markup slugged the old way and names
  none of those fragments.

- `content { collections { .. { paginate { mount } } } }` and `prefix` are
  validated the way `permalink` always was. A `..` in either was previously
  accepted and produced wrong URLs out of a green build; a placeholder spelling
  that means nothing (`prefix "{n}"`) was silently ignored and is now an error.

- `deploy { ssh { .. } }` now requires `host` and an absolute `path`, and
  `deploy { s3 { .. } }` requires a non-empty `bucket`. Both were optional and
  defaulted to nothing, so an incomplete block deployed the site to `/` on the
  host and issued `create_dir("/assets")`. Name where the site was actually
  landing:

  ```kdl
  deploy {
    ssh {
      host "srv.example"
      path "/var/www/site"
    }
  }
  ```

  The check runs before anything connects, so a wrong block fails without a
  build behind it.

- A config key that takes one value out of a fixed set now refuses a second one.
  `links { style "clean" "flat" }` used to drop `"flat"` in silence. Nothing
  legitimate is refused: every such key reads exactly one value. Write the one
  you meant.

- Two config diagnostics have their own codes rather than borrowing
  `baudelaire::config::unknown_value`, which described neither: a `serve
  { editor }` that is a command line rather than a program is now
  `baudelaire::config::command_line`, and an `html { footnotes }` naming no
  element is `baudelaire::config::not_an_element`. Only a consumer keying on the
  code is affected.

### Added

- **markdown**: `.md` content pages, behind the default-on `markdown` feature.
  CommonMark plus the GFM set, with frontmatter in a fenced block at the top of
  the file:

  ```md
  ---
  title: Hello
  tags: [rust, typst]
  ---

  Ordinary **prose**.
  ```

  The fence says which language the block is in, so a post copied out of another
  generator needs no rewriting: `---` is YAML, `+++` is TOML, and `;;;` is KDL,
  the language `config.kdl` uses. Nothing after the fence names it, and no
  dialect has a second spelling, so a block is never read as a language it is
  not. All three are read into the same dict a `.typ` page exports, and nothing
  downstream knows which one it came from.

  KDL is the one that cannot spell a one-element list (a single argument is
  always the scalar, the counterpart of Typst's `("rust",)`); YAML and TOML
  both can, so a post with exactly one tag has a spelling now.

  It is a source dialect, not a second pipeline: a markdown page lowers to Typst
  before it compiles, so permalinks, taxonomies, link resolution, highlighting,
  sidecars and the incremental cache are the ones that were already there.

  A fence's info string carries `key` or `key=value` parameters after the
  language, and `eval` is the one that means something today: it runs the block
  as Typst rather than showing it, which is how a markdown page reaches a
  template helper. A fence with no `eval` is always a sample, so a page can
  document Typst without running it.

  ````md
  ```typ         a sample, shown
  ```typ eval    run as Typst
  ````

  Raw HTML other than a comment is refused rather than dropped: the DOM is
  typed, and a string of markup has nowhere to be spliced into it. Write it as
  `html.elem("div")[..]` in an `eval` fence.

  Whether `.md` is a page at all, and what one may contain, is the site's, in
  `content { markdown { } }`. `markdown #false` is the shorthand for
  `markdown { enabled #false }`, for a project whose binary has markdown and
  which still wants its `.md` files left alone:

  ```kdl
  content {
    markdown {
      enabled #true                 // `markdown #false` is the shorthand
      extensions "smart" "-tables"  // `-name` drops a default
      html "drop"                   // or `refuse`, the default
      eval #false                   // forbid `eval` fences outright
    }
  }
  ```

  `eval #false` is the one to reach for on content you did not write: an `eval`
  fence runs arbitrary Typst at build time. `html "drop"` removes an inline
  run's tags and keeps the prose between them; a block-level run is a single
  chunk of HTML, so dropping it drops its contents too, which is why `refuse` is
  the default.

- **markdown**: a typst error inside an `eval` fence is reported against the line
  you wrote, not against the Typst the page lowered to. Only authored spans are
  mapped, so an error in generated code still reports where it really is rather
  than pointing at a line of yours that has nothing to do with it.

  ```
  x unclosed delimiter
    ,-[content/broken.md:9:10]
  8 | ```typ eval
  9 | #let x = [
    :          -
  ```

- **init**: an interactive run asks what to start from, offering the four
  starter shapes and the four themes the binary carries, each with the line that
  describes it. Naming one on the command line (`-t` for a starter shape,
  `--theme` for a theme) answers the question and skips the prompt, and `-y`
  takes the default shape, as before.
  Choosing a theme writes it into `themes/<name>` and names that directory in
  the config, so the answer is a site that builds rather than one with a theme
  still to find.

- A frontmatter `date` may be written as an ISO day (`date: "2024-01-01"`), not
  only as `datetime(year: .., month: .., day: ..)`. KDL has no date literal, so
  a markdown page needs the string form; a typst page gets it too, since both
  read through one reader. Exactly `YYYY-MM-DD` is accepted, so a string that
  merely resembles a date cannot silently become one.

- **A section turned on by its presence can be turned back off on its own line.**
  `lint #false`, `caching #false`, `generate { robots #false }`, `navigation
  { spa #false }` and ten siblings, where naming the section at all used to be
  the only thing a config could say about it. It is the spelling
  `content { markdown #false }` already had. This is what a profile or a theme's
  `theme.kdl` had no way to express: an overlay applies nodes over the base, so
  naming the section is what re-enables it, and the language has no spelling for
  deleting a node.

- **cli**: `--color=auto|always|never`. It layers over the existing detection
  rather than replacing it, and an explicit choice beats every environment
  signal, so `--color=never` wins over `CLICOLOR_FORCE`. It is read before clap
  writes `--help`, so that output is coloured to match too.

- **cli**: `-q` counts. `-qq` leaves only diagnostics and the exit code.

- **cli**: `--help` lists the exit codes it returns and the environment
  variables it reads.

- A `deploy { s3 { endpoint } }` or `announce { standard { pds } }` written as
  `http://` is now reported, because the secret sent to it travels in clear. A
  warning rather than a refusal, since a local MinIO or PDS is how both get
  developed against.

- A deploy or announce that stops partway now says where it stopped, and an
  announce keeps the skip-cache it had built, so a re-run resumes instead of
  starting over.

### Fixed

- A frontmatter key that is a typo, and a frontmatter value of the wrong type,
  now underline the line you wrote instead of only naming it. Neither error
  carried a source snippet at all, on a markdown page or a typst one, though the
  walk that raises them always knew where the field was:

  ```
  x unknown frontmatter key `titel` in content/a.md
    ,-[content/a.md:2:1]
  1 | ---
  2 | titel: A
    : ----+---
    :     `-- no such frontmatter key
    `----
    help: did you mean `title`?
  ```

  A typst page that computes its frontmatter has no literal to point at, and
  there the snippet is still suppressed rather than aimed at a guess.

- A section reached by its shorthand is recorded as configured, so the feature
  gate on it can fire. `content { markdown }` on a binary built without the
  `markdown` feature dropped every `.md` page and said nothing at all: the
  shorthand never marked the section present, and the gate reads present *and*
  enabled. It now warns, as every other gated capability already did.

- **markdown**: a typst error in a bound template is reported against the
  template. It was translated through the *page's* source map whenever the
  template was at least as long as the page's lowered body, so a typo in a
  template underlined a line of prose in some short `.md` page and never named
  the template at all.

- **markdown**: a closing fence has to be alone on its line, as the opening one
  already did. `--- and more` closed a block and left `" and more"` as the first
  bytes of the body, silently. A file written with CRLF also labels its
  unterminated block on the whole fence rather than one byte to the left of it.

- **markdown**: an indented `eval` fence maps each of its lines. The block's own
  indentation was stripped from the first line alone, so every diagnostic inside
  a fence nested in a list item pointed one column off, growing with the depth.

- A diagnostic snippet is tagged with the language the page is actually written
  in. All three sites that render one said `Typst`, which was true until a `.md`
  file became a page.

- **markdown**: `html { spans }` stamps a markdown page with the line you wrote.
  It stamped the lowered Typst instead, under a virtual path
  (`content/a.md@layout:3:2` for prose on line 6), so the dev server's alt-click
  asked the editor to open a file that does not exist. Every construct is mapped
  now, nested ones included: a list item, and a link inside a list item, each
  name their own line and column.

  The map that does it moved out of the error module and covers the whole
  lowering rather than `eval` fences alone. Both readers go through it, so a
  diagnostic and a stamp cannot disagree about where a page came from.

- **markdown**: a link naming a markdown page resolves to its permalink, and is
  checked. The test for "is this a source path" was the literal `typ`, so a
  `.md` target was classified as a URL: it was published as the relative path
  the author wrote and never verified, so a green build served a dead link. This
  project's own documentation shipped one. The extensions that count as source
  paths are now the site's, so where markdown is off (or absent from the binary)
  a `.md` link is still the file link it was written as.

  A translated markdown page is found too: the edition probed alongside the
  target was spelled `{stem}.{lang}.typ` whatever the target's own extension
  was, so `hello.fr.md` was never looked for and every reader got the original.

- **serve**: a session no longer rebuilds itself forever. Templates `#import`
  the generated `sections.typ`, so the build recorded it as a file it read and
  the watcher watched it -- and every build rewrites it, so every build queued
  the next one. A session span at roughly one rebuild per 140ms, producing
  nothing, until it was killed. Nothing under the scratch directory triggers a
  rebuild now; those files are derived from content and templates, which are
  watched, so the edit that changes one already rebuilds on its own account.

- **config**: a profile is checked when the config is parsed, not when it is
  selected. A profile block was retained as raw KDL and dispatched only on
  selection, so a key no scope has (`content { drafts #true }` written as
  `content { draft #true }`) parsed green, reloaded green under `serve`, and
  configured nothing at all whenever the profile was finally used.

- **config**: a `${VAR}` inside a profile nobody selected no longer fails the
  build. Checking every profile at parse time meant resolving its values too, so
  the `${S3_BUCKET}` that the docs recommend putting in a `prod` profile broke
  `baudelaire build` for everyone who had not exported it. The check now reads
  the shape and stands a placeholder in for an unset variable; selecting the
  profile still demands the real thing. A byte size is the one shape no
  placeholder satisfies, so `${..}` in a `lint { budget { .. } }` key inside an
  unselected profile still fails.

- **render**: a heading's id is slugged from its text in document order. The
  walk read an element's own text before the text nested inside it, so any
  heading containing emphasis, a link, `raw`, math or a footnote marker was
  slugged from a scramble: `== The *fast* way` produced `the-way-to-a-fast`.
  Since the deep-link check reads the same ids, a link written correctly against
  the heading as authored failed the build.

- **render**: a responsive `srcset` names an image's directory once. A candidate
  was built by appending a name that already carried its authored directories to
  a URL that carried them too, so an image in a content subdirectory promised
  `/assets/gallery/gallery/cover-480.png` while the file was written one
  `gallery` shallower. Every downscale 404'd and only the full-size fallback
  loaded. Images directly under `content/` were unaffected, which is why every
  scenario missed it.

- **render**: a markdown page's links reach the link graph. A lowered page's body
  is inlined into its wrapper, and the test for "is this the page's own content"
  asked whether the file was a `.typ`, so no link on a `.md` page was ever
  recorded: `page.backlinks` never named a markdown source, and
  `links { orphans }` called a page unlinked that a markdown page linked to.

- **render**: `<meta name="author">` and the social vocabularies name the same
  person. One read the site-wide author and the other the language-aware one, so
  a site with `languages { fr { author .. } }` published two different authors on
  the same page.

- **render**: a page's recorded anchors come out in a stable order, so two
  identical builds write the same manifest.

- **render**: a link's scheme is read at its head. `://` anywhere in the href
  counted, so `b.typ?redirect=https://x` was treated as external and published
  as an unresolved `.typ` path.

- **markdown**: raw HTML wrapped in comments is refused like any other. A run
  beginning `<!--` and ending `-->` was taken for a comment whatever sat between
  them, so `<!-- a --><div>x</div><!-- b -->`, which CommonMark reads as one HTML
  block, was dropped silently: content lost, and the default `html "refuse"`
  bypassed. Genuinely empty comments (`<!-->`, `<!--->`) are now read as comments
  too, where the old length guard rejected them.

- **markdown**: an email autolink keeps its scheme. `<me@example.com>` published
  `href="me@example.com"`, a relative link to a page that does not exist, which
  the link checker had no reason to flag. `[x](mailto:..)` on the same page was
  correct, so the two spellings disagreed.

- **markdown**: an image's alt text keeps the space between its lines, instead of
  running `alpha\nbeta` together as `alphabeta`.

- **markdown**: a `$` run is lowered as the text it is. Math is not enabled, so
  the arms that would have handled it were unreachable, and they would have
  flattened an equation into literal characters if they ever ran.

- **markdown**: a YAML frontmatter error whose position falls past the end of the
  block underlines the last thing you wrote, rather than the closing fence.

- **engine**: `page.reading` counts a markdown page's words. The estimate skipped
  every line beginning `#`, which is every line the lowering emits, so a markdown
  page reported zero words and the shipped themes rendered "0 min read" on all of
  them. It now counts the markdown you wrote, fenced code excluded.

- **engine**: a markdown page inside a `generate { pdf { bundle } }` is bound as
  the Typst it lowered to. The bundle had no case for one, so it handed the raw
  `.md` to the compiler: a page with a heading failed the build, and a page
  without one shipped its markdown verbatim with its frontmatter lost.

- **engine**: `page.assets` lists a page bundle's assets only. It skipped `.typ`
  alone, so a sibling `.md` page, and the page's own source, were offered as
  assets at URLs that are never published.

- **engine**: a slim binary says that it is ignoring `.md` files. The warning
  asked whether the site had written `content { markdown }`, but the documented
  way to use markdown is to write nothing at all, so the pages simply vanished
  and `prune` removed the HTML an earlier build had written. It now asks whether
  the site has any.

- **engine**: a markdown page's backlink prediction reads the Typst it lowered
  to, rather than parsing its `.md` as Typst and finding nothing.

- **serve**: editing a `.md` page rebuilds. The watcher took only `.typ` under
  `content/` and `templates/`, so a markdown edit produced no rebuild and no
  reload until something else was touched. The same test dropped every non-`.typ`
  dependency in those trees, so a colocated `data.json` a page read, or an image
  in a page bundle, was equally invisible.

- **serve**: the banner names the roots that are actually there, and a watch root
  that does not exist is skipped by the same predicate that decides what to
  advertise.

- **cli**: `--json` leaves `completions`, `man` and `reference` alone. The report
  was written to stdout after those commands had written their document there,
  so `baudelaire completions bash --json > _bd` produced a file with a JSON
  object stapled to the end of the script.

- **cli**: `mirror` says when it could not read the config. It fell back to the
  defaults in silence, so the `site` module an editor resolved against carried a
  title and url the project had never set, while `build` failed on the same file.

- **cli**: `new` says when it could not read the existing content. The error was
  discarded, taking the next `order` and the permalink-collision check with it.

- **cli**: `init -y` writes a placeholder author rather than `author ""` when git
  has no `user.name`, which used to reach every page's `<meta>` and every feed.

- **cli**: `theme` prints its output rather than its markup. `theme list`, `add`
  and `info` wrote diagnostic markup straight to the terminal, so the backticks
  and asterisks showed, and a path containing one was escaped on screen.

- **cli**: several diagnostics named something that does not work. The unpinned
  DID warning gave a dotted config key the parser rejects; the missing-VCS
  warning suggested `init --vcs`, which is a usage error; a `serve` open request
  with no location was reported as a malformed one; and every failure to spawn
  the VCS was reported as the tool not being installed.

- **cli**: `theme info` lines up its labels again, including the two that are
  wider than the column was.

- An image sized in typst renders identically whether `assets { images { extract
  } }` is on or off. With it on the sizing CSS is reproduced rather than reused,
  because typst's encoder is private, and the reproduction was wrong three ways:
  a negative term was added rather than subtracted (`width: 50% - 10pt` came out
  `calc(50% + -10pt)`), a ratio was rounded to four decimals where typst rounds
  to two (`33.3333%` against `33.33%`), and the properties were written in
  authored order without spaces where typst sorts them and writes `name: value`.
  A page genuinely rendered two ways depending on the setting.

- A downscaled variant of an image under an absolute `paths { assets }` is found
  rather than externalized a second time.

- The JSON islands the standalone export and the JSON-LD tag write now escape
  every `<`, not only `</`. A value carrying `<!--<script` opened the
  script-data-double-escaped state, after which the island's own `</script>` did
  not close it and the rest of the document was swallowed.

- A title carrying a control character no longer makes `rss.xml` and
  `sitemap.xml` unparseable. Those characters are forbidden outright in XML and
  have no character reference, so they are dropped at the writer.

- A value carrying a line break no longer writes a line of its own into
  `_redirects`, `_headers`, `robots.txt` or `llms.txt`, and a space in a
  `_redirects` path no longer splits its record. The five line-oriented formats
  now go through one writer, as the XML and script outputs already did.

- **theme**: an archive larger than the 64 MiB ceiling fails, naming the limit,
  instead of installing a truncated copy.

- **deploy**: an SSH key file that is missing or unreadable is reported as such.
  Every failure to load one was read as "encrypted", so a wrong path prompted
  for a passphrase that could not help.

- **deploy**: the host-key remedy names the `known_hosts` entry that was
  actually checked. On any port but 22 the entry is written `[host]:port`, so
  the `ssh-keygen -R <host>` both diagnostics printed matched no line and
  removed nothing.

- **deploy**: paths a remote's own listing named that a deploy cannot act on are
  reported instead of dropped. Such a key is invisible to `--delete` and to the
  summary alike, so it sat on the remote for ever with nothing saying it was
  there.

- **cli**: `deploy` and `announce` check that a destination is configured before
  building the site, rather than after.

- **cli**: `clean --dry-run` lists what would actually be removed, and a
  declined prompt says it was declined instead of `nothing to clean`.

- **cli**: sizes are labelled `KiB`/`MiB`/`GiB`/`TiB`, which is what they have
  always been measured in. `Bytes` still parses `kB` and `MB` as 1024, so no
  configured budget changes meaning.

- **cli**: `--strict-links` says what it checks, which is a `.typ` link to a
  missing page or heading, not every internal link.

- **serve**: `?at=` with an empty value reports no source location rather than a
  malformed one, which rendered as a message ending in a bare colon.

- **serve**: an XML or SVG file is served with the charset it is encoded in.
  Only `text/*` carried one, though XML defines the parameter and JSON does not.

### Security

- **A theme unpacked from a `.tar.gz` can no longer write outside the project.**
  The zip reader refused an entry naming `..` or an absolute path; the tar reader
  had no such check and the install joined the entry onto the theme directory,
  where an absolute path discards the directory entirely. The escaped file was
  then recorded in the lock, so `theme update` overwrote it and `theme remove`
  deleted it: create, overwrite and delete anywhere the user could write, from
  `baudelaire theme add <url>`. Both formats now decide containment in one place.
  A theme name derived from an archive's own wrapper directory is checked too,
  which closes `..` reaching the same join.

- **A theme can no longer run commands on `baudelaire build`.** A theme's
  `theme.kdl` was parsed as a full config and used as the floor beneath the
  site's, so a site that stated no `hooks` of its own inherited the theme's, and
  `hooks { before }` runs through the system shell. See the note under
  **Upgrading** for what is refused and why.

- `gh:owner/..` and its siblings are refused when a forge shorthand is parsed,
  rather than relying on the archive reader to catch the name later.

## [0.0.11] - 2026-08-05

### Upgrading

- `Renderer::SCHEMA` is bumped (14 -> 16), so the first build after upgrading is
  a cold one. Extracted images are named differently, and a warm manifest would
  replay the flat names into cached pages while fresh ones wrote the new paths.
  If you link an extracted image's URL by hand anywhere (a feed, an external
  page), it now carries the directories the image sits in under `content/`. A
  page also records the URLs it links to by name, which no earlier manifest
  carries: see the link-graph fix below.

- `url` must be an absolute URL, scheme and all. `url "example.com"` used to
  build green and then publish `<loc>example.com/a/</loc>` in the sitemap, the
  same in every feed `<id>`, canonical tag and `og:url`. Any scheme is accepted:
  this is the site readers are served, not a host credentials are sent to.
  `--base-url` answers to the same rule.

- A `{ }` block written on a setting that takes `key=value` attributes is now an
  error instead of being ignored. It never configured anything, so a build that
  goes red here was already not doing what its config said:

  ```kdl
  assets { images { optimize { png { level 6 } } } }   // was silently level 2
  assets { images { optimize { png level=6 } } }       // what it has to say
  ```

  The four scopes are `assets.images.optimize.png`, `.jpeg`,
  `content.taxonomies` and `generate.manifest.icons`. The generated reference
  called them blocks, so it documented the spelling that did nothing; it now
  names them `key=value ..` and `named lines`.

- A site whose `assets/` holds preprocessor sources, `_partial` files or
  unbundled `.ts` will stop publishing them. That is the point, but if you were
  linking one of those URLs deliberately, move the file to `static/`, which
  publishes verbatim. The documented Tailwind recipe now names its input
  `assets/_app.css` so the input stays out of the output.

### Added

- **search**: the indexed region of a page is the site's to name, rather than
  always the `<main>` landmark:

  ```kdl
  generate {
    search {
      region "article"      // the element whose contents are indexed
      ignore "nav" "aside"  // dropped wherever they appear inside it
    }
  }
  ```

  Both are tag names, matched whole and case-insensitively, with nesting counted.
  `region ""` indexes the whole page. The default is unchanged (`main`, nothing
  ignored), and a page without the named element is still indexed whole.

  Nesting is counted, which the old `<main>`-only scan did not do: it ended at
  the first closing tag it saw. That was invisible while the region was always
  `main`, which does not nest, and would have truncated every page with an
  `<article>` inside an `<article>`.

- **theme**: `theme add` takes a spec rather than one of four names, and fetches
  from wherever it points: a directory on this machine (`./plume`), the Typst
  package store (`@preview/plume:1.0.0`), a repository on a forge
  (`gh:owner/plume#v1.2.0`, `gl:`, `cb:`, `sr:`, a GitHub or Codeberg URL, or
  `forgejo:host/owner/plume` for a self-hosted instance), or an archive over http
  (`.tar.gz`, `.tgz`, `.zip`). A repository or an archive that holds a whole
  project takes `--subdir <path>`, naming the theme inside it.

  The copy records where it came from, so `theme update` goes back to the same
  source without being told again, and keeps your edits exactly as before. A
  repository copy records the ref it follows and what the forge served for it.
  `theme list` grew a second section for the copies this binary does not ship,
  each with the source it came from.

  A repository is fetched as the forge's own source archive of one revision, not
  cloned: the same files in one request, and no git on the host or in the
  binary.

- **collections**: `feed`, a syndication feed of one collection's members.

  ```kdl
  content {
    collections {
      posts { feed #true; paginate { template "list.typ" } }
    }
  }
  ```

  Written beside the collection's index (`/posts/rss.xml`) in every configured
  format, carrying only that collection's dated pages under the same `limit`,
  and advertised in each member's `<head>` beside the site feed. The site feed
  carries everything dated, which is the wrong granularity for a site that
  publishes more than one kind of thing: a reader who wants the essays had to
  take the release notes too. Per collection rather than one flag over all of
  them, because most collections want no feed at all. A feed's home is the index
  it sits beside, so the collection needs `paginate { }`, and one mounted at `/`
  is told the site feed keeps that file rather than being overwritten by the
  narrower of the two.

- **config**: a top-level `redirect { }` block, for the old paths no page owns.

  ```kdl
  redirect {
    "/blog/page/1/" "/blog/"
    "/tags/rs/" "/tags/rust/"
    "/shop/" "https://shop.example.com"
  }
  ```

  Frontmatter `redirect` speaks for a page that still exists. A paginated
  `page/1/` another generator wrote, a renamed term listing and a section you
  deleted have no frontmatter to declare anything in, and until now nothing could
  claim those paths back. Old path first, destination second; both sides literal,
  and a destination may name another host. Stubs or `_redirects` rules exactly as
  a frontmatter `redirect` produces, under the same `generate { redirects }`
  switch. A pair aiming at a path some page already publishes fails the build
  rather than burying that page under a stub forwarding away from it.

- **frontmatter**: `path`, the URL a page publishes at, replacing its
  collection's permalink pattern and its slug both.

  ```typ
  #let frontmatter = (
    title: "The night train",
    path: "/2019/03/night-train.html",
  )
  ```

  Leading and trailing slashes are optional; a path whose last segment carries an
  extension names a file and publishes as one, rather than as a directory with an
  `index.html` in it. It is the escape hatch a migration needs: a site arriving
  from Hugo (`url`), Zola (`path`), Jekyll or Eleventy (`permalink`) can copy each
  old URL onto the page that answers it and match the old URL set exactly, rather
  than reverse-engineering one pattern per shape. Two pages claiming one path is
  an error naming both files. Identity is untouched, so translations still pair on
  `collection/slug` and each edition may state its own path.

- **templates**: a page knows itself. `page.url` is where it publishes,
  `page.collection` which collection it belongs to, and `page.assets` maps the
  files beside a page bundle to the URLs they are served from, so a frontmatter
  `hero: "cover.png"` finally resolves:

  ```typ
  #let page(page, body) = {
    let hero = page.frontmatter.at("hero", default: none)
    if hero != none { h("img", src: page.assets.at(hero), alt: "") }
    body
  }
  ```

  Each names only the page itself, never another page, so it widens that page's
  own cache identity and no one else's; the rule that keeps the site tree out of
  the wrapper is unchanged. `page.assets` is empty for a page that shares its
  directory with its neighbours.

- **serve**: the dev server watches what the build read. A page or template that
  loads a file outside `content/`, `templates/`, `assets/` and `static/` (a
  `data/authors.yaml`, a CSV a table is built from) is watched because the build
  recorded reading it, so editing that file rebuilds the pages that read it.
  `serve { include }` is unchanged and still names what no compile reads: a
  `tsconfig.json`, a hook's input, a directory that is empty so far.

- **init**: every starter shape writes a `content/404.typ`, and a build with no
  not-found page says so once (`baudelaire::content::not_found`, advice). A site
  without one hands unmatched URLs to whatever its host answers with, which was
  easy to not notice until a visitor found it.

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

  Five verbs, and `--dir` on each to put the theme somewhere other than
  `themes/<name>`:

  | | |
  |---|---|
  | `list` | The four, each marked with where it is installed and how many files you have edited. |
  | `add` | Write it in and name the `config.kdl` line. |
  | `info` | Its templates, the collections and taxonomies its `theme.kdl` declares, and the state of your copy. |
  | `update` | Rewrite the files you have not touched from this binary. |
  | `remove` | Delete the files still baudelaire's. |

  `add` leaves a `.baudelaire-lock.json` beside the files recording what each
  digests to in that binary, which is what `update` and `remove` read to tell
  your edits from ours: an edited file is kept and reported (`--force`
  overrides), a deleted one stays deleted, and a file that was never the
  theme's is never touched. It is the only lockfile baudelaire has, and it
  exists because a theme is *vendored* rather than resolved; a published theme
  (`@namespace/name:1.0.0`) is pinned by its own spec through Typst's package
  store.

  `init --theme "themes/albatros"` writes the theme as part of the scaffold. The
  `themes` cargo feature carries them (~230 KiB); a `slim` binary has neither the
  themes nor the command.

- **build**: a template nothing supplies is one typed diagnostic
  (`baudelaire::template::missing`) naming the file, what asked for it (a config
  key, or the page whose frontmatter named it) and where to write it, raised
  before the first compile. It used to be typst's own `file not found`, once per
  page, pointing at the generated wrapper.

- **init**: every scaffold writes a `.gitignore` for `public/` and `.baudelaire/`,
  not only the runs that set up a repository with `--vcs`.

### Fixed

- **cli**: `--version` names the `themes` feature. It was missing from the table
  that report reads, so a build carrying the `theme` command and nothing else
  called itself `slim`, which is the one thing that report exists to answer.

- **themes**: a theme installed as a Typst package renders. Its layouts were
  imported as a package subpath (`@local/plume:0.1.0/templates/page.typ`), which
  typst reads as a version, so every page of a package-themed site failed with
  `0/templates/page is not a valid patch version` while the theme's assets and
  `theme.kdl` came through as usual. The package's own root is now served under
  the project, so an import is a path again and a theme's relative imports
  (`../parts.typ`, a `show raw` palette) resolve as they do in a directory theme.

- **theme**: every verb looks where the config's `theme` line says the theme is,
  not only in `themes/<name>`. A theme installed anywhere else was invisible to
  `theme list`, and `info`, `update` and `remove` reported it as not installed
  unless you repeated the `--dir` you had used to add it.

- **theme**: `--dir` outside the project is refused instead of written. The
  build cannot use a theme there (a Typst import cannot leave the root), so the
  files it wrote could never be read.

- **theme**: `theme add` records only the files it wrote. It re-recorded the
  whole shipped set, digested from the running binary, so a second `add` over an
  install from an earlier baudelaire disowned every file that binary had written:
  each untouched one then read as edited, and `theme update` kept it, release
  after release. A file that was already there when `add` ran is now never
  claimed at all, so `theme remove --force` no longer deletes a file baudelaire
  did not write.

- **serve**: the watcher is registered before the banner says it is. It came up
  after the banner and after the browser launch, so an edit saved in that window
  reached nobody; a file event is edge-triggered, so nothing later made up for
  it and the session ignored that save for good. The window is invisible on a
  small site and seconds long on one whose first build is slow.

- **deploy**: a bucket that refuses a request says why. The S3 agent surfaced a
  non-2xx as a transport error, so the branch that reads the bucket's own
  `<Error><Code>` body never ran and a wrong secret, a missing bucket and a rate
  limit all reported the same bare failure.

- **deploy**, **announce**: every outbound request has a deadline. Only the link
  checker had one, so a `deploy` or `announce` against a black-holed endpoint
  blocked for ever with nothing to cancel. A bucket listing also stops after
  10,000 pages instead of following continuation tokens indefinitely, matching
  the ceiling the announce record walk already had.

- **templates**: a frontmatter value that is not a string reaches JavaScript as
  itself. `weight: 3`, `featured: true`, `authors: ("ada", "bob")` and a nested
  dict all arrived in `baudelaire:pages` as `null`, against the declared
  `Record<string, unknown>`, because every non-string Typst value was carried as
  a Typst-only expression. The same values written in `client { }` always
  arrived intact, which is what made it look like a typing problem rather than a
  conversion one. `wants_card` sees through them too, so a page whose `image:`
  was not written as a plain string literal no longer has a card drawn over it.

- **embed**: an asset the inliner could not read is recorded as a dependency all
  the same, so a page referencing a file that was not there inlines it once it
  appears. It stayed a cache hit instead, and the self-contained export kept
  pointing at a file it does not carry.

- **frontmatter**: frontmatter derived from a build input the file tracker
  cannot see (a git hash, `datetime.today()`) re-derives when that input
  changes. Discovery cached the extracted value against the files the evaluation
  read and nothing else, so the value was frozen at the build that first
  extracted it: a title naming the current commit named the first one for ever.
  The page's compile invalidated correctly and re-emitted the stale value it was
  handed, which is what made it look like tracking was working.

- **frontmatter**: a page whose frontmatter reads `@baudelaire/pages` or
  `@baudelaire/sections` re-derives it once the table exists. Discovery runs
  before the build has written the table, so a cold build legitimately sees the
  empty one; the read was then dropped from the page's cache entry rather than
  recorded as an absence, so that first answer was carried forward for every
  later build. A title counting the site's pages said zero for ever.

- **meta**: a base path prefixes the meta tags that carry a URL, and leaves the
  rest alone. `content` counted as a URL on every `<meta>` in the page, so a
  site hosted under a subpath prefixed its own prose: a page titled
  `/etc/hosts, annotated` published `og:title` as `/docs/etc/hosts, annotated`,
  and the same for `og:description`, `article:tag` and the `twitter:` pair.

- **cards**: a card template a theme ships is the one the card draws with. The
  check that a template exists accepted either layer's, but the card built its
  import path out of the project's `templates/` alone, so a site relying on the
  theme's `card.typ` passed the check and then failed every card-bearing page
  with `file not found`.

- **links**: a link an author wrote as a URL (`#link("/guide/")`) is an edge of
  the link graph whether or not the page exists yet. The page writing it now
  records that it asked, so adding a page at that URL rebuilds the linker and
  the new page gains its backlink. It used to record only the links that
  matched: with `links { backlinks }` or the orphan report on, the linker stayed
  a cache hit, its recorded edges did not include the new page, and the report
  called that page linked from nowhere out of a green build. Deleting the target
  left the opposite: an edge to a page no longer there.

- **config**: the generated reference names the spelling that parses. A
  free-entry table (`languages.strings`, `client`, `redirect`, `typst.inputs`,
  `html.highlight`) reads child nodes and was labelled `key=value ..`, which is
  a KDL parse error; it is `key value ..`. The four `key=value` scopes were
  labelled `block`, which is the spelling they discard.

- **images**: an image extracted from a page keeps the directories it was
  authored under, so `posts/a/cover.png` and `posts/b/cover.png` are two files
  (`/assets/posts/a/cover.png` and `/assets/posts/b/cover.png`). They were served
  flat under the base name, which is one name for both: a page bundle tree, where
  naming a picture for its role is the convention every Markdown generator
  encourages, collided on every post and served one picture for all of them, with
  a warning each. A width variant is also spliced on the file name rather than
  the whole path, so a directory holding a dot no longer corrupts it.

- **assets**: the pipeline publishes what it produced, not what it read. A
  default-config site copied its own TypeScript to `dist` (`app.ts`, which no
  browser runs, comments and all), its Sass and Less sources, and the
  `_partial` files the bundler's own convention already treats as import-only,
  because that convention applied only while bundling. One rule now covers the
  whole asset tree: a leading `_`, a `.d.ts` declaration, a preprocessor source
  (`.scss`, `.sass`, `.less`, `.styl`), and an unbundled script source
  (`.ts`, `.tsx`, `.jsx`) are inputs and stay out of the output. `static/` is
  untouched, being the verbatim escape hatch.

- **serve**: an unmatched URL under a language's own subtree is answered with
  that language's not-found page. The build writes one per language and a host
  picks by directory; the dev server always served the default language's, so a
  French page's broken link previewed in English.

- **build**: adding a file to a starter shape or a bundled theme rebuilds the
  binary that carries it. Both trees are embedded with `include_dir!`, which
  records no dependency on what it walked, so a *new* file silently stayed out of
  the next build and `init` wrote the shape as it was two builds ago.

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

[unreleased]: https://github.com/cestef/baudelaire/compare/v0.0.12...HEAD
[0.0.12]: https://github.com/cestef/baudelaire/compare/v0.0.11...v0.0.12
[0.0.11]: https://github.com/cestef/baudelaire/compare/v0.0.10...v0.0.11
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
