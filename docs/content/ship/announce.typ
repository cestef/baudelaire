#let frontmatter = (
  title: "Announcing",
  order: 4,
)
#import "/templates/theme.typ": callout

`baudelaire announce` publishes your site's _metadata_ to the
#link("https://atproto.com")[AT Protocol] as
#link("https://standard.site")[standard.site] records: one
`site.standard.publication` for the site, one `site.standard.document` per dated
page.

```kdl
url "https://example.com"

announce {
  standard {
    handle "you.bsky.social"
  }
}
```

Then `baudelaire announce --dry-run` to see the plan, and `baudelaire announce`
to write it.

It doesn't upload the built files. That's #link("deploy.typ")[deploying]. This
tells the network your pages exist and where they live.

#callout(kind: "note")[
  Needs the `announce` cargo feature, which the default build has and the `slim`
  flavor drops. See #link("../start/install.typ")[install].
]

== Configure

Only `handle` is required. A top-level `url` is too, since every record points
at a real address.

#table(
  columns: 4,
  align: (left, left, left, left),
  table.header([Key], [Type], [Default], [Does]),
  [`handle`],
  [str],
  [--],
  [The account the records are written under. Required.],

  [`did`],
  [str],
  [resolved],
  [Pins the repository DID, and unlocks the verification artifacts below.],

  [`pds`],
  [url],
  [`https://bsky.social`],
  [The host to authenticate and write records against.],

  [`discover`],
  [bool],
  [`#true`],
  [Opts the publication into standard.site discovery surfaces.],

  [`icon`],
  [path],
  [--],
  [A file under the project root, uploaded as the publication's icon.],
)

A page becomes a document when its
#link("../write/frontmatter.typ")[frontmatter] carries a `date`. standard.site
requires a publication date, so undated pages are reported as skipped and the
run warns with the count. Title, `description` (or `summary`), and
#link("../write/collections/taxonomies.typ")[taxonomy] terms travel with each document.

== The app password

Authentication uses an *app password*, never your account password. On Bluesky
(the default PDS), create one under Settings, Privacy and Security, App
Passwords: add one, name it `baudelaire`, copy the `xxxx-xxxx-xxxx-xxxx` value.
It's revocable and scoped, so it's safe to hand to a build. On another PDS, get
one from that provider.

The password never goes in config. It's resolved from the first of these that
has it:

```sh
# 1. --secret, with `-` reading stdin so it stays out of the process arguments
echo "$APP_PASSWORD" | baudelaire announce --secret -

# 2. the environment variable, best for CI
BAUDELAIRE_ATPROTO_PASSWORD="xxxx-xxxx-xxxx-xxxx" baudelaire announce

# 3. a hidden prompt, when a terminal is attached
baudelaire announce
```

#callout(kind: "warn")[
  `--secret xxxx-...` with the value spelled out lands in your shell history and
  the process list. Use stdin or the environment variable anywhere shared.
]

== Running it

`announce` builds the site first, so a run always reflects current sources, then
reconciles the remote repository with your pages.

```sh
baudelaire announce --dry-run   # show the plan, write nothing
baudelaire announce             # confirm, then write
baudelaire announce --yes       # skip the confirmation
```

The remote is the source of truth. Each run puts new and changed records, skips
unchanged ones, and deletes document records whose page no longer exists, so
nothing is orphaned. The summary line counts all three.

#callout(kind: "tip")[
  `--dry-run` needs no app password. It lists your live repository over public
  reads to compute an accurate plan, then writes nothing.
]

In CI, pass `--yes`. Without a terminal to confirm at, the run fails rather than
skipping the backend and exiting 0.

== Domain verification

Set `did` to your repository DID and the build emits verification artifacts
offline, no announce required:

```kdl
announce {
  standard {
    handle "you.bsky.social"
    did "did:plc:abc123.."
    verify {
      wellknown #true
      links #true
    }
  }
}
```

#table(
  columns: 3,
  align: (left, left, left),
  table.header([Key], [Default], [Writes]),
  [`wellknown`],
  [`#true`],
  [`/.well-known/site.standard.publication`, holding the publication's `at://` URI.],

  [`links`],
  [`#true`],
  [A `link rel="site.standard.document"` in each dated page's head.],
)

Both prove the site and the records belong together. A site may emit one, the
other, or both, and neither is written unless `did` is set.

With `did` set, `announce` resolves your handle and refuses a mismatch, so you
can't write under the wrong identity. `--dry-run` runs the same check, catching
a bad `did` before you authenticate.

#callout(kind: "tip")[
  Don't know your DID? Run `baudelaire announce --dry-run` without it. The
  output prints the resolved value to configure, no password needed.
]
