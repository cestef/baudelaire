#let frontmatter = (
  title: "GitLab Pages",
  order: 6,
)
#import "/templates/theme.typ": callout

GitLab Pages publishes the `public/` artifact of a job named `pages`, which is
exactly what `baudelaire build` produces. The whole of `.gitlab-ci.yml`:

```yaml
pages:
  image: alpine:latest
  before_script:
    - apk add --no-cache curl
    - curl -fsSL https://baudelaire.cstef.dev/install.sh | sh
  script:
    - ~/.local/bin/baudelaire build
  artifacts:
    paths: [public]
  cache:
    key: baudelaire
    paths: [.baudelaire]
  rules:
    - if: $CI_COMMIT_BRANCH == $CI_DEFAULT_BRANCH
```

The installer picks the musl build on a musl host and the glibc one otherwise,
so Alpine and Debian images both work with the same line. Only `curl` (or
`wget`) and `tar` have to be present.

== The job name

The job must be called `pages` and its artifact directory must be `public`. Both
are GitLab's requirements, and `public` is already baudelaire's default `dist`,
so nothing needs changing. If you moved it:

```kdl
paths {
  dist "public"
}
```

== Project sites live under a path

A project site is served at `https://GROUP.gitlab.io/PROJECT`. GitLab hands the
job that URL as `CI_PAGES_URL`, so pass it straight through instead of hardcoding
it:

```yaml
  script:
    - ~/.local/bin/baudelaire build --base-url "$CI_PAGES_URL"
```

Every root-absolute link, feed entry and sitemap URL then carries the `/PROJECT`
prefix. Set `url` in `config.kdl` as well, so a local build matches.

== Caching

`.baudelaire/` holds the #link("../../build/incremental.typ")[incremental] build
state. The `cache:` block above restores it, so a rebuild only recompiles what
changed. Use a per-branch key if branches diverge often:

```yaml
  cache:
    key: baudelaire-$CI_COMMIT_REF_SLUG
    paths: [.baudelaire]
```

#callout(kind: "note")[
  `artifacts` and `cache` are different things here. GitLab publishes the
  artifact and only restores the cache, so `public` belongs in one and
  `.baudelaire` in the other. Swapping them publishes nothing.
]

== Pushing instead

To upload somewhere else from the same pipeline, drop the `pages` name and the
artifact and run #link("../deploy.typ")[`baudelaire deploy`] with the credentials
as masked CI variables:

```yaml
deploy:
  image: alpine:latest
  before_script:
    - apk add --no-cache curl
    - curl -fsSL https://baudelaire.cstef.dev/install.sh | sh
  script:
    - ~/.local/bin/baudelaire deploy --yes
  rules:
    - if: $CI_COMMIT_BRANCH == $CI_DEFAULT_BRANCH
```

`deploy` builds the site itself, so there is no separate build step. The
destination reads `AWS_ACCESS_KEY_ID` and `AWS_SECRET_ACCESS_KEY` (see
#link("s3.typ")[S3]) or `BAUDELAIRE_SSH_PASSWORD` (see #link("ssh.typ")[SSH])
from the environment, which is what a CI variable already is.
