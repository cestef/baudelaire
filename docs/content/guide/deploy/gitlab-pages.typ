#let frontmatter = (
  title: "GitLab Pages",
  order: 12,
)

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
