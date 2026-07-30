# Changelog

Notable changes to baudelaire. Format follows [Keep a Changelog][kac]; versions
follow [Semantic Versioning][semver], with the pre-1.0 caveat that a breaking
change bumps the patch number.

Nothing that only affects the repository is listed: refactors, tests, CI and
chores are visible in the git history and change nothing for a site.

[kac]: https://keepachangelog.com/en/1.1.0/
[semver]: https://semver.org/spec/v2.0.0.html

## [Unreleased]

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
- **deploy**: `deploy { s3 { cache { } } }` sets `Cache-Control` on upload.
  Content-addressed files get `max-age=31536000, immutable`, everything else
  revalidates; the split is derived from `assets { fingerprint }`, which is what
  makes hashing a filename worth anything at the last step.
- **deploy**: The SSH backend is behind a default-on `ssh` cargo feature. It is
  the most expensive thing in the tree after typst and rolldown by crate count.

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

[Unreleased]: https://github.com/cestef/baudelaire/compare/v0.0.7...HEAD
[0.0.7]: https://github.com/cestef/baudelaire/compare/v0.0.6...v0.0.7
[0.0.6]: https://github.com/cestef/baudelaire/compare/v0.0.5...v0.0.6
[0.0.5]: https://github.com/cestef/baudelaire/compare/v0.0.4...v0.0.5
[0.0.4]: https://github.com/cestef/baudelaire/compare/v0.0.3...v0.0.4
[0.0.3]: https://github.com/cestef/baudelaire/compare/v0.0.2...v0.0.3
[0.0.2]: https://github.com/cestef/baudelaire/compare/v0.0.1...v0.0.2
[0.0.1]: https://github.com/cestef/baudelaire/releases/tag/v0.0.1
