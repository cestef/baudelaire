#let frontmatter = (
  title: "S3-compatible storage",
  order: 8,
)
#import "/templates/theme.typ": callout

`baudelaire deploy` uploads the built files to an S3-compatible bucket: AWS S3,
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
  default host; set it for R2, MinIO, or any non-AWS provider. Must be `https`,
  since the request carries your signature; `http://localhost` is allowed for a
  local MinIO.
/ `region` (`us-east-1`): region code. R2 uses `auto`.
/ `prefix`: a key prefix (subdirectory in the bucket) every object lands under.
/ `delete` (`#true`): remove objects under `prefix` that the build no longer
  produces, so the bucket mirrors `public/` exactly.

Change detection is stateless: S3 returns each object's ETag, which for a
single-part upload is the MD5 of its bytes, so a file whose content already
matches is skipped without any local record to keep or lose. Re-deploying an
unchanged site uploads nothing.

== Credentials

Baudelaire never stores credentials in config. The access key id is read from
`AWS_ACCESS_KEY_ID`; the secret key is resolved, in order of precedence, from:

```sh
# 1. stdin (--secret -), so it never appears in the process arguments;
#    this wins over the environment variable when both are present
echo "$AWS_SECRET_ACCESS_KEY" | baudelaire deploy --secret -

# 2. the AWS_SECRET_ACCESS_KEY environment variable, best for CI
AWS_ACCESS_KEY_ID=AKIA.. AWS_SECRET_ACCESS_KEY=.. baudelaire deploy

# 3. an interactive prompt (hidden input) when a terminal is attached
AWS_ACCESS_KEY_ID=AKIA.. baudelaire deploy
```

`--secret` also accepts a literal value, which takes precedence over everything
above, but it lands in the process arguments; prefer `--secret -` or the
environment variable.

Temporary credentials work as they are: `AWS_SESSION_TOKEN`, set by GitHub OIDC,
an EC2/ECS instance role, `aws sso login` or `sts assume-role`, is picked up and
signed along with the request.

== Cache headers

A bucket serves whatever `Cache-Control` you set on an object, and by default
that is none at all. Add a `cache` block and every upload gets one:

```kdl
deploy {
  s3 {
    bucket "my-site"
    cache { }
  }
}
```

Files under the asset prefix are sent as
`public, max-age=31536000, immutable`, and everything else as
`public, max-age=0, must-revalidate`. Override either:

```kdl
cache {
  immutable "public, max-age=604800, immutable"
  default   "public, max-age=300"
}
```

The split is what `assets { fingerprint }` is *for*: hashing a filename after
its content means a changed file has a different name, so the old one can be
cached forever without ever going stale. Without fingerprinting on, an asset
keeps its authored name across builds and is exactly as mutable as a page, so it
gets the revalidating policy too. Baudelaire works that out from your build's
own config rather than asking you to restate it.

#callout(kind: "note")[
  SFTP has nowhere to put a header, so this is S3-only. On an SSH target the
  headers are your web server's to set.
]
