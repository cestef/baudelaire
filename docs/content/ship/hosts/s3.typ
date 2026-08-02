#let frontmatter = (
  title: "S3-compatible storage",
  order: 9,
)
#import "/templates/theme.typ": callout

`baudelaire deploy` uploads the built files to AWS S3, Cloudflare R2, MinIO, or
anything else that speaks the S3 API. The block's presence turns the destination
on:

```kdl
deploy {
  s3 {
    bucket "my-site"
  }
}
```

```sh
AWS_ACCESS_KEY_ID=AKIA... AWS_SECRET_ACCESS_KEY=... baudelaire deploy --yes
```

== Keys

#table(
  columns: 4,
  align: (left, left, left, left),
  table.header([Key], [Type], [Default], [Does]),
  [`bucket`], [str], [--], [The bucket uploaded into. Required.],
  [`endpoint`],
  [url],
  [--],
  [The API host, for a non-AWS provider. Unset targets AWS.],

  [`region`],
  [str],
  [`us-east-1` / `auto`],
  [The region the request is signed under.],

  [`prefix`],
  [str],
  [--],
  [A key prefix every object goes under. Unset uploads at the bucket root.],

  [`delete`],
  [flag],
  [`#true`],
  [Remove objects under `prefix` that the build no longer produces.],
)

Set `endpoint` for anything that is not AWS. It selects path-style addressing
and, unless you state a `region`, signs the request as `auto`, which is what R2
and most S3-compatible hosts want:

```kdl
deploy {
  s3 {
    bucket "my-site"
    endpoint "https://ACCOUNT.r2.cloudflarestorage.com"
    prefix "site"
  }
}
```

Without an `endpoint`, AWS virtual-hosted addressing is used and the region
defaults to `us-east-1`. State `region` when your provider expects a real region
code.

#callout(kind: "warn")[
  The endpoint must be `https`: the request carries your signature.
  `http://localhost` is the one exception, for a MinIO on your own machine.
]

== Credentials

Nothing secret goes in `config.kdl`. The access key id is read from
`AWS_ACCESS_KEY_ID`. The secret key is resolved in this order:

#table(
  columns: 2,
  align: (left, left),
  table.header([Source], [When to use it]),
  [`--secret -` (stdin)], [Piping a secret without exposing it in `argv`.],
  [`AWS_SECRET_ACCESS_KEY`], [CI, where a variable is already the mechanism.],
  [An interactive prompt], [A terminal, with neither of the above set.],
)

```sh
# stdin wins over the environment variable
echo "$SECRET" | baudelaire deploy --secret -

# CI: both from the environment, no prompt possible
AWS_ACCESS_KEY_ID=... AWS_SECRET_ACCESS_KEY=... baudelaire deploy --yes
```

A literal `--secret <value>` beats all three, and lands in your shell history.
Prefer stdin.

Temporary credentials work unchanged. `AWS_SESSION_TOKEN`, set by GitHub OIDC,
an EC2 or ECS instance role, `aws sso login` or `sts assume-role`, is picked up
and signed with the request.

== Change detection

S3 returns each object's ETag, which for a single-part upload is the MD5 of its
bytes. Baudelaire hashes the local files the same way and compares, so a file
whose content already matches is skipped. Nothing is recorded locally, which is
why re-deploying an unchanged site from a fresh checkout still uploads nothing.

`delete` mirrors the other direction: an object under `prefix` that the build no
longer produces is removed, so the bucket ends up matching `public/` exactly.
Turn it off if something else writes into the same prefix:

```kdl
deploy {
  s3 {
    bucket "my-site"
    delete #false
  }
}
```

== Cache headers

A bucket serves whatever `Cache-Control` you put on an object, and by default
that is nothing at all. Declare a policy and every upload gets one:

```kdl
caching { }
```

Fingerprinted assets are sent as `public, max-age=31536000, immutable`, and
everything else as `public, max-age=0, must-revalidate`. Override either:

```kdl
caching {
  immutable "public, max-age=604800, immutable"
  default   "public, max-age=300"
}
```

#table(
  columns: 3,
  align: (left, left, left),
  table.header([Key], [Type], [Does]),
  [`immutable`],
  [str],
  [The value for content-addressed assets, which can be cached forever.],

  [`default`], [str], [The value for everything else.],
)

The block sits at the top level, not under `deploy`, because it describes the
built site rather than one destination. The same policy drives the
#link("static-hosts.typ")[`_headers` file], so a site that does both cannot
state two different answers to one question. It is not `cache { }`, which
configures the #link("../../build/incremental.typ")[build cache].

The split is what #link("../../build/assets.typ")[`assets { fingerprint }`] is for.
Hashing a filename after its content means a changed file has a different name,
so the old one can be cached forever without going stale. With fingerprinting
off, an asset keeps its authored name across builds and gets the revalidating
policy like any page. Baudelaire reads that from your build's own config rather
than asking you to restate it.

#callout(kind: "note")[
  Headers are set on S3 uploads only. SFTP has nowhere to put one, so on an
  #link("ssh.typ")[SSH target] they are your web server's to set.
]
