#let frontmatter = (
  title: "Deploying",
  order: 7,
)
#import "/templates/theme.typ": callout

`baudelaire build` writes a plain folder of static files to `public/` — the
`dist` directory from #link("config.typ")[config], overridable with `--out`.
There is no server to run. Host it anywhere that serves files.

== Built-in deploy with `deploy`

`baudelaire deploy` uploads the built files to an S3-compatible bucket — AWS S3,
Cloudflare R2, MinIO, or anything that speaks the S3 API. Add a `deploy` block
with your bucket:

```kdl
deploy {
  s3 {
    bucket "my-site"
  }
}
```

Every other key has a default:

/ `bucket`: bucket name. Required.
/ `endpoint`: an S3-compatible host, e.g.
  `https://ACCOUNT.r2.cloudflarestorage.com`. Unset targets AWS at the region's
  default host; set it for R2, MinIO, or any non-AWS provider.
/ `region` (`us-east-1`): region code. R2 uses `auto`.
/ `prefix`: a key prefix (subdirectory in the bucket) every object lands under.
/ `delete` (`#true`): remove objects under `prefix` that the build no longer
  produces, so the bucket mirrors `public/` exactly.

Change detection is stateless: S3 returns each object's ETag, which for a
single-part upload is the MD5 of its bytes, so a file whose content already
matches is skipped without any local record to keep or lose. Re-deploying an
unchanged site uploads nothing.

=== Credentials

Baudelaire never stores credentials in config. The access key id is read from
`AWS_ACCESS_KEY_ID`; the secret key is resolved, in order, from:

```sh
# 1. the environment variable — best for CI
AWS_ACCESS_KEY_ID=AKIA.. AWS_SECRET_ACCESS_KEY=.. baudelaire deploy

# 2. stdin, so it never appears in the process arguments
echo "$AWS_SECRET_ACCESS_KEY" | baudelaire deploy --secret -

# 3. an interactive prompt (hidden input) when a terminal is attached
AWS_ACCESS_KEY_ID=AKIA.. baudelaire deploy
```

=== Deploying

`baudelaire deploy` builds the site first, so a run always reflects the current
sources, then reconciles the bucket with `public/`:

```sh
baudelaire deploy --dry-run   # list the bucket, show the plan, write nothing
baudelaire deploy             # confirm, then upload and delete
baudelaire deploy --yes       # skip the confirmation
```

The remote is the source of truth. Each run uploads new or changed files, skips
matching ones, and (with `delete`) removes objects the build no longer produces.

=== SSH / SFTP

`baudelaire deploy` also targets any host reachable over SSH, transferring files
with SFTP. Add an `ssh` block instead of (or alongside) `s3`:

```kdl
deploy {
  ssh {
    host "example.com"
    path "/var/www/site"
    key "~/.ssh/id_ed25519"
  }
}
```

/ `host`: server hostname or IP. Required.
/ `path`: absolute remote directory the build is mirrored into. Required.
/ `port` (`22`): SSH port.
/ `user`: user to authenticate as. Defaults to `$USER`.
/ `key`: path to a private key (absolute, or under the project root). Unset falls
  back to password authentication.
/ `delete` (`#true`): remove remote files under `path` that the build no longer
  produces.

Authentication is key-based when `key` is set — supply the passphrase, if the key
has one, the same way as any other secret (below). Without `key`, a password is
resolved from `BAUDELAIRE_SSH_PASSWORD`, stdin (`--secret -`), or the prompt.

Change detection runs `sha256sum` on the host and diffs it against the local
files, so an unchanged file is never re-sent. If the host cannot run it (a bare
directory, a non-coreutils system), every file simply uploads — the deploy is
still correct, just not incremental that run.

== Any static host

Upload `public/` to Netlify, Vercel, Cloudflare Pages, GitHub Pages, S3, or your
own nginx. The typical settings:

/ Build command: #raw("baudelaire build")
/ Publish directory: #raw("public")

Hosts that build for you (Netlify, Vercel, Cloudflare Pages) have no Rust
toolchain, so install the prebuilt binary first in the build command:

```sh
curl -fsSL https://baudelaire.cstef.dev/install.sh | sh && ~/.local/bin/baudelaire build
```

#callout(kind: "tip")[
  With clean URLs on, a page lives at `posts/hello/index.html`. Most hosts serve
  `index.html` for a directory automatically; if yours doesn't, point its
  "pretty URLs" or rewrite option at `index.html`.
]

== GitHub Pages

Set #emph[Settings → Pages → Source] to #emph["GitHub Actions"], then commit this
workflow. It installs the prebuilt binary (no compile), caches the incremental
build state, and hands `public/` to Pages. The `permissions`, `environment`, and
`concurrency` blocks are all required for `deploy-pages` to work.

```yaml
name: deploy
on:
  push: { branches: [main] }

permissions:
  contents: read
  pages: write
  id-token: write

concurrency:
  group: pages
  cancel-in-progress: true

jobs:
  deploy:
    runs-on: ubuntu-latest
    environment:
      name: github-pages
      url: ${{ steps.deploy.outputs.page_url }}
    steps:
      - uses: actions/checkout@v7
      - name: Install baudelaire
        run: |
          curl -fsSL https://baudelaire.cstef.dev/install.sh | sh
          echo "$HOME/.local/bin" >> "$GITHUB_PATH"
      - uses: actions/cache@v6
        with:
          path: .baudelaire
          key: baudelaire-${{ hashFiles('content/**', 'assets/**', 'config.kdl') }}
          restore-keys: baudelaire-
      - run: baudelaire build
      - uses: actions/upload-pages-artifact@v5
        with: { path: public }
      - id: deploy
        uses: actions/deploy-pages@v5
```

#callout(kind: "note")[
  Prefer `cargo install baudelaire` in CI only if you already cache the Cargo
  build — a cold compile is minutes, the prebuilt binary is seconds. On
  non-`x86_64`/`aarch64` runners the installer has no binary, so fall back to
  `cargo install`.
]

== GitLab Pages

GitLab Pages serves the `public/` artifact of a job named `pages` — which is
exactly what `baudelaire build` produces. The installer picks the glibc or musl
binary to match the image, so Alpine works too:

```yaml
pages:
  image: alpine:latest
  script:
    - apk add --no-cache curl
    - curl -fsSL https://baudelaire.cstef.dev/install.sh | sh
    - ~/.local/bin/baudelaire build
  artifacts:
    paths: [public]
  cache:
    key: baudelaire
    paths: [.baudelaire]
  rules:
    - if: $CI_COMMIT_BRANCH == $CI_DEFAULT_BRANCH
```

== Forgejo / Gitea Actions

The GitHub workflow above runs almost verbatim on a Forgejo or Gitea runner —
keep the `build` and `upload`/`deploy` steps, and swap the Pages actions for
whatever your instance provides (many just publish the `public/` artifact).

== Announcing with `announce`

`baudelaire announce` is not a deploy step — it uploads none of `public/`. It
_announces_ your site to the #link("https://atproto.com")[AT Protocol] via
#link("https://standard.site")[standard.site]: a publication record plus one
document record per dated page, written to the PDS in your config. Deploy the
files with one of the methods above, then run `announce` to broadcast them to the
network. See #link("../features/build/announcing.typ")[Announcing].

== Preview builds

Point the canonical URL at the preview host so feeds and the sitemap use the
right absolute links:

```sh
baudelaire build --base-url "https://preview.example.com"
```

Because builds are #link("../features/build/incremental.typ")[incremental], caching
the `.baudelaire/` directory between CI runs makes rebuilds near-instant — as the
workflows above do.
