# Security

## Reporting a vulnerability

Report privately, not in a public issue.

- Preferred: [open a draft advisory](https://github.com/cestef/baudelaire/security/advisories/new).
  The discussion stays private until there is a fix to publish.
- Or email `root@cstef.dev`.

Please include what you were running (`baudelaire --version`), what you did, what
happened, and what you expected. A minimal site that reproduces it helps most.

Expect an acknowledgement within a few days. This is a single-maintainer
project, so fixes are best-effort and have no schedule, but you will be told
where things stand. You are credited in the advisory and the changelog unless
you prefer not to be.

## Supported versions

Only the latest release. Baudelaire is pre-1.0, and fixes ship in the next
release, not as patches to older ones.

## What is in scope

Baudelaire is a static site generator: a build tool run by a site's own author
over that author's own files. This decides what counts as a vulnerability.

In scope:

- Site content escaping the build: a `.typ` page, a config file or a theme
  causing writes outside `dist`, arbitrary command execution, or a read of a
  file outside the project root.
- Credential leaks: a deploy or announce secret reaching the terminal, a log,
  a diagnostic, `--json` output, or the built site. Baudelaire handles an S3
  access key and secret, an SSH key or password (`BAUDELAIRE_SSH_PASSWORD`), and
  an atproto app password (`BAUDELAIRE_ATPROTO_PASSWORD`).
- Generated-output injection: authored content escaping its context in the
  emitted HTML, feeds, sitemap, or search index in a way the author could not
  have produced deliberately.
- The dev server (`baudelaire serve`) doing something reachable from another
  machine: path traversal out of `dist`, or the live-reload channel doing more
  than reloading. It binds `127.0.0.1` by default.
- A dependency advisory that is reachable from baudelaire's code. `cargo deny
  check` runs in CI; `deny.toml` lists the ones already assessed and why.

Out of scope:

- A site's own content doing what its author told it to. Typst is a programming
  language, and a `.typ` file in your `content/` is code you chose to run, like
  a `build.rs`. Building someone else's untrusted `.typ` file is outside the
  threat model.
- `hooks { }` running the commands it is configured to run.
- The dev server exposed deliberately with `serve { bind "0.0.0.0" }`. It has no
  authentication and is not built to face a network.
- The installer fetching over HTTPS from GitHub releases. The published `.sha256`
  files show a transfer was not corrupted. They are not signatures and do not
  prove authorship.
- Missing hardening with no exploit path: a lint, a header baudelaire does not
  set by default, or a dependency advisory that no reachable code path hits.
