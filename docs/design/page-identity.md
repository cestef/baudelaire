# Page identity: separating what a page *is* from where it *sits*

## The problem

Baudelaire currently expresses three different questions with two overlapping answers.

| Question | Today's answer | Stable under rename? |
|---|---|---|
| Which page is this? (identity) | `PageId` = `collection/slug` | No |
| What is it right now? (content) | `graph::Hash` (blake3) | No, by design |
| Where does it sit? (position) | source path, permalink, `PageId` | No, by design |

`content/page.rs` calls `PageId` a "stable identifier for a page within the site". It is
stable across *languages* (that is what it was built for: translation matching pairs
`about.typ` with `about.de.typ` because both key to `pages/about`). It is not stable
across time. Rename the file, or set a `slug` in frontmatter, and the page's identity
silently becomes a different page's identity.

Position is doing identity's job in three places, and each one pays for it:

1. **Announce.** `announce/standard/record.rs` computes the atproto record key as
   `Rkey::derived(&doc.path)`, an FNV-1a hash of the page's permalink. Renaming a page
   produces a different rkey, so the reconcile pass in `Standard::reconcile_documents`
   sees one record that should exist and one that should not: it PUTs a new record and
   DELETEs the old. The `at://` URI changes. Anything that referenced the old record now
   points at nothing.
2. **Redirects.** A rename is invisible to the build. The author must notice, and
   hand-write `redirect: ("/old/",)` into the frontmatter, or the old URL 404s. The
   build has every fact needed to know a move happened and cannot express it.
3. **Cache.** `graph/cache.rs` keys the manifest `pages: BTreeMap<PathBuf, Entry>`. A
   moved file with byte-identical content misses the cache and recompiles, because the
   only handle the cache has on the page is where it used to live.

This is the jj insight applied to content: a change has a stable identity across
rewrites, while the commit hash churns. Baudelaire already reaches for it (translation
identity is `collection+slug`, cache validity is a content hash) but never separates the
axes, so identity keeps collapsing back into position.

## The inversion that makes this cheap

jj needs an operation log because a commit is an immutable object: you cannot write a
stable id into a thing you are about to replace, so the id lives beside the object and
rewrite operations carry it forward.

A `.typ` file is not immutable. It is edited in place and moved with `git mv`. So the id
can live **inside the object**, and every rewrite carries it for free, with no log, no
heuristics, and no VCS coupling. This is strictly stronger than jj's model: the identity
survives leaving version control entirely (a tarball, a copied directory, a fresh clone
at depth 1).

Only one thing genuinely needs a log, and it is not identity: *history*. "What URL did
this page used to have" is not recoverable from the current tree at any price. That, and
only that, justifies a persisted ledger.

## Design

### Three types, three questions

- **`Uid`** (new): identity. 128 random bits, assigned once, never derived from content
  or location. Lives in frontmatter as `id`.
- **`graph::Hash`** (exists): content. Unchanged.
- **`PageKey`** (renamed from `PageId`): position. `collection/slug`, with its doc
  comment corrected to say it is a positional key and explicitly not stable across
  renames.

The rename is churn across roughly eight files, but the current name is actively
misleading: it is what let position stand in for identity in the first place.

### Where the uid lives

Frontmatter, one row in the existing `FIELDS` dispatch table in
`content/frontmatter.rs`:

```typ
#let frontmatter = (
  id: "k7m2q9x4pb3nf8vd6rjw5tzhya",
  title: "Hello",
)
```

Properties this buys, none of which a sidecar or VCS-derived scheme can match:

- Survives `git mv`, `jj move`, a manual rename in a file manager, and a fresh CI clone.
- No rename *detection*. There is nothing to guess: the id is right there.
- Works with no VCS at all.
- Diffable and reviewable. A changed id shows up in review as a changed line.

**Compatibility note:** unknown frontmatter keys currently pass through as `extra`, so a
site already using `id:` for its own purposes would find it captured by the parser. This
needs a release note, and `id` should be rejected as a non-string with a precise error
rather than silently ignored.

### Format

26 characters, Crockford base32, lowercase, of 128 random bits.

- Legal as an atproto rkey with no encoding (charset `[A-Za-z0-9.\-_:~]`, max 512).
- Filename-safe and URL-safe, so it can key a cache directory or an anchor.
- No visually ambiguous characters, so it survives being read aloud or retyped.

`getrandom` is already in the dependency graph unconditionally (via russh), so this adds
no new build cost, and none at all to the slim flavor.

Rejected: ULID. Time-ordering is redundant with the `date` frontmatter the site already
sorts on, and a ULID embeds the authoring timestamp into a value that gets published as
a public record key. Small, but it is a fact leaking for no gain.

### Who assigns it (the hard invariant)

**The build never writes to source files.** Not in `build`, not in `serve`, not in
`check`. This is not a preference:

- A build that rewrites content is not deterministic, and the whole cache design assumes
  it is.
- Under `serve`, a build that writes into the watched tree retriggers itself. That is an
  infinite rebuild loop, and the watcher filter in `cli/serve.rs` would have to grow a
  special case to paper over it.
- A build that mutates the working tree makes CI dirty and makes review noisy.

So assignment happens at exactly two places, both explicit:

- `baudelaire new` stamps a fresh uid into the page it scaffolds. Free, no new UX.
- `baudelaire id` (new subcommand) backfills every unstamped page, printing a diff-style
  summary and touching nothing else. This is the migration path for an existing site.

A page with no uid behaves exactly as it does today, at every one of the three sites.
The whole feature is opt-in per page, which satisfies "generated pages are opt-in or
user-overridable, never opinionated defaults".

### The ledger (needed only for redirects)

`baudelaire.lock`-shaped, at the project root, committed:

```kdl
// uid -> permalinks it has occupied, newest first
page "k7m2q9x4pb3nf8vd6rjw5tzhya" {
  permalink "/posts/hello-world/"
  permalink "/posts/hello/"
}
```

Semantics deliberately copied from `Cargo.lock`, because the tension is identical (a
build artifact that must be committed):

- `baudelaire build` updates it in place.
- `baudelaire check` reports drift and **fails**, so CI catches a missing commit. This
  is the `--locked` equivalent and makes the file's staleness a build error rather than
  a silent wrong redirect.
- `serve` must exclude it from the watch filter. `cli/serve.rs` already centralises the
  watched roots in `Filter::roots`, so this is one entry, not a special case sprinkled
  around.

Opt-in via a `history { }` config block. With it absent, no ledger is read or written
and redirects stay fully manual.

Rejected: deriving old permalinks from git/jj history. It is slow, couples the build to
two VCS CLIs, and silently returns nothing under `actions/checkout`'s default
depth-1 clone, which is the exact environment where a wrong answer ships.

## What each payoff actually looks like

### 1. Announce

`document_uri(did, path)` becomes `document_uri(did, uid)`, and the rkey is the uid
verbatim (no hashing: it is already rkey-legal and already opaque).

- A stamped page that moves keeps its `at://` URI. The record's *content* updates with
  the new URL; the record's *identity* does not move. Subscribers keep their reference.
- An unstamped page keeps `Rkey::derived(path)`. No behaviour change.
- `SkipCache` (`announce/mod.rs`) currently keys by rkey string; it moves to keying by
  uid. Changing the key means one full re-send on first run after upgrade, which the
  module's own doc already declares harmless: sends are idempotent and deletions diff
  against the remote, never against the cache.

**Honest cost:** the migration itself breaks URIs once. atproto has no rename; moving a
record from the path-derived rkey to the uid rkey is a create plus a delete. The
sequence should be PUT-new-then-DELETE-old so a failure leaves a duplicate rather than a
gap, and it should be gated behind an explicit confirmation in the announce summary,
listing exactly which records will move. After that one migration, renames are free
forever.

### 2. Redirects

For each stamped page, compare the current permalink against the ledger's newest entry.
On change: emit a redirect stub for **every** historical permalink, all pointing at the
current one, and prepend the new permalink to the ledger.

- Emitting from every historical entry rather than just the previous one means no
  redirect chains: `/a/` and `/b/` both land directly on `/c/`, one hop each.
- Auto stubs go through the same `Claim` path as manual `redirect` frontmatter in
  `content/mod.rs`, so: prune leaves them alone (they are claimed outputs), and a
  collision between an auto stub and a real page is caught by the existing uniqueness
  check instead of one silently overwriting the other.
- Language scoping must reuse the existing rule. `engine/emit/redirect.rs` already
  carries a subtlety about unlocalized old paths producing stubs in both scopes; the
  auto path must go through that same code, not a parallel copy of it.
- Manual `redirect` frontmatter keeps working and composes: it is for URLs that were
  never this page's permalink (an import from a previous site), which no ledger can
  know.

### 3. Cache

`pages: BTreeMap<PathBuf, Entry>` keys on the uid when present, else the path.

**Honest scope of the win:** the reusable work is the moved page's own typst compile,
which is the expensive part per page. That is real but bounded. Two things it does *not*
fix, which should not be claimed:

- Changing the permalink *pattern* in config changes the config hash and rebuilds
  everything, by design. Identity does not and should not change that.
- Other pages that link to the moved page still re-render, because their output genuinely
  changed.

The point is narrower than "renames become free": keyed by path, per-page reuse across a
move is not even *expressible*. Keyed by identity, it is.

## New failure modes

These are the real cost of the feature and each needs a precise typed diagnostic, per
`error/fs.rs`:

- **Duplicate uid.** Copying a `.typ` file copies its id. Two pages claiming one
  identity must be a hard error at discovery, labelling both source paths. This is the
  most likely mistake and the one that would corrupt announce state silently if allowed
  through.
- **Malformed uid.** Wrong length, wrong charset, wrong type. Hard error, never a
  silent fall back to positional identity.
- **Ledger drift under `check`.** Its own error class, with a help line naming the
  command to run.

## Open decision: exposing the uid to templates

`page.id` in a template would give authors a stable anchor and a stable DOM id. But it
means stamping a page changes its rendered bytes, so the backfill command becomes a
site-wide content change rather than a metadata-only one.

Recommendation: expose it. The alternative is a value the build knows and the author
cannot see, and the one-time rebuild is exactly what a one-time backfill should cost.

## Rollout

Each phase is independently shippable and none changes behaviour for an unstamped site.

1. **`Uid` type, frontmatter field, duplicate/malformed detection, `new` stamps,
   `baudelaire id` backfills.** No consumer yet. Ships the identity without spending it.
2. **Cache keys on uid.** Smallest blast radius, entirely local, easiest to verify.
3. **Announce rkey.** Needs the one-time migration path and its confirmation prompt.
4. **Ledger and auto-redirects.** The only phase that adds a committed artifact and a
   new config block.

## Parked, with reasons

- **Translations stay on `PageKey`.** The uid answers "same page across time", not
  "same page across languages", and those are different questions. Renaming `about.typ`
  and `about.de.typ` together keeps them matched under position-based identity anyway,
  so there is no bug here to fix. A `translation` frontmatter field naming another
  page's uid would be a second mechanism for knowledge the file layout already carries.
- **Uid in the content fingerprint.** It must not be. Cache validity is a question about
  content; making it depend on identity inverts the whole split this spec exists to
  draw.

## Touch list

| File | Change |
|---|---|
| `src/content/uid.rs` | new: `Uid` (generate, parse, Display, Serialize) |
| `src/content/page.rs` | `PageId` -> `PageKey`; add `Page::uid: Option<Uid>` |
| `src/content/frontmatter.rs` | one row in `FIELDS` |
| `src/content/history.rs` | new: ledger load / diff / append |
| `src/error/content.rs` | duplicate uid, malformed uid |
| `src/graph/cache.rs` | manifest key |
| `src/announce/standard/record.rs` | `document_uri` takes a uid |
| `src/announce/mod.rs` | `SkipCache` keys by uid |
| `src/engine/emit/redirect.rs` | ledger-derived stubs |
| `src/cli/scaffold.rs` | `new` stamps |
| `src/cli/id.rs` | new: backfill subcommand |
| `src/cli/serve.rs` | ledger excluded from the watch filter |
| `src/config/mod.rs` | `history { }` block |
