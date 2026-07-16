#let frontmatter = (
  order: 13,
  title: "Announcing",
  tags: ("feature", "announcing"),
)
#import "/templates/theme.typ": callout

Baudelaire is atproto-native. `baudelaire announce` publishes your site's
_metadata_ to the #link("https://atproto.com")[AT Protocol] as
#link("https://standard.site")[standard.site] records — a
`site.standard.publication` for the site and one `site.standard.document` per
dated page. It doesn't upload the built files (that's
#link("../../guide/deploy.typ")[deploying]); it announces, to the network, that
your pages exist and where they live.

The layer is backend-neutral — a destination is one `impl Backend` — but
standard.site is the one that ships, and the one this is built around.

== Configure

Enable it by adding an `announce` block. Only `handle` is required — it is the
account the records are written under.

```kdl
announce {
  standard {
    handle "you.bsky.social"
  }
}
```

Every other key has a default:

/ `handle`: account handle or DID to authenticate as. Required.
/ `pds`: the host to authenticate and write records against. Defaults to
  `https://bsky.social`; set it if your account lives on another PDS.
/ `did`: your repository DID, a stable public identifier (not a secret). Setting
  it turns on the build-time verification artifacts below.
/ `discover` (`#true`): opt the publication into discovery surfaces.
/ `icon`: a path under the project root, uploaded as the publication's icon blob.

A page becomes a document when its frontmatter has a `date`; standard.site
requires a publication date, so undated pages are reported as skipped. Title,
description (or `summary`), and taxonomy terms travel with each document.

== The app password

Authentication uses an *app password*, never your account password. On Bluesky
(the default PDS), create one at
#link("https://bsky.app/settings/app-passwords")[Settings → Privacy and Security
→ App Passwords]: add a password, name it (e.g. `baudelaire`), and copy the
`xxxx-xxxx-xxxx-xxxx` value. It is revocable and scoped, so it is safe to hand to
a build. On another PDS, get the app password from that provider instead.

Baudelaire never stores the password in config. It is resolved, in order, from:

```sh
# 1. the environment variable — best for CI
BAUDELAIRE_ATPROTO_PASSWORD="xxxx-xxxx-xxxx-xxxx" baudelaire announce

# 2. stdin, so it never appears in the process arguments
echo "$APP_PASSWORD" | baudelaire announce --password -

# 3. an interactive prompt (hidden input) when a terminal is attached
baudelaire announce
```

#callout(kind: "warn")[
  A password on the command line as `--password xxxx-..` is visible in your shell
  history and the process list. Prefer the environment variable or `--password -`
  in any shared or automated environment.
]

== Announcing

`baudelaire announce` builds the site first, so a run always reflects the current
sources, then reconciles the remote repository with your pages:

```sh
baudelaire announce --dry-run   # show the plan, write nothing
baudelaire announce             # confirm, then write
baudelaire announce --yes       # skip the confirmation
```

The remote is the source of truth. Each run puts new or changed records, skips
unchanged ones, and deletes document records whose page no longer exists, so
nothing is orphaned. A dry run lists the remote to compute an accurate plan — it
just writes nothing.

#callout(kind: "note")[
  `--dry-run` needs no app password: it diffs against your live repository over
  public reads to tell you exactly what a real run would send and remove.
]

== Domain verification

standard.site can tie a publication to your own domain. Set `did` to your
repository DID and the build emits verification artifacts offline — no announce
required:

```kdl
announce {
  standard {
    handle "you.bsky.social"
    did "did:plc:abc123.."
    verify {
      wellknown #true   // /.well-known/site.standard.publication
      links #true       // per-page <link rel="site.standard.document">
    }
  }
}
```

Both artifacts prove the site and the records belong together; a site may emit
one, the other, or both. When `did` is set, `announce` resolves your handle and
refuses a mismatch, so you never write under the wrong identity — and `--dry-run`
runs the same check, catching a misconfigured `did` before you ever authenticate.
Don't know your DID yet? Run `announce --dry-run` without it — the output prints
the value to configure, no password required.
