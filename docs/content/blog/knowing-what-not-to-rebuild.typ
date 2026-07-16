#let frontmatter = (
  title: "Knowing what not to rebuild is the whole game",
  date: datetime(year: 2026, month: 7, day: 2),
  tags: ("blog", "internals", "rust"),
  summary: "Turning one Typst file into HTML is the easy part. The hard part is not doing it 500 times because you fixed a typo. How Baudelaire tracks dependencies through an opaque compiler, hashes with blake3, and content-addresses its output.",
)
#import "/templates/theme.typ": callout

Compiling one `.typ` file into one `.html` file is a solved problem: you hand it
to the Typst compiler and you're done. The interesting problem in a static site
generator isn't compiling a page. It's _not_ compiling the other 499 pages when
you fixed a typo in one of them.

The naive version is "compare modification times," and it falls apart fast. Edit a
shared layout and every page that uses it is now stale, but nothing on _their_
mtimes changed. A page imports a theme module that loads a CSV; touch the CSV and
some pages need a rebuild, but which ones? You can't answer that from the
filesystem, because the dependency lives inside the Typst source, and to see it
you'd have to parse the Typst source, which is exactly the work you're trying to
avoid.

So the job is: figure out, cheaply and correctly, the smallest set of pages a
change can possibly affect.

== Hash the input, not the clock

First, stop trusting mtimes and start trusting content. Every page's source gets
fingerprinted with #link("https://github.com/BLAKE3-team/BLAKE3")[blake3], and the
fingerprint is compared against the one stored from last build. Same bytes, same
hash, candidate for reuse. That covers `touch`-ing a file, a `git checkout` that
rewrites mtimes, saving without editing.

#callout(kind: "note")[
  The `Hash` type stores the raw 32 bytes, not the 64-char hex string. Cache
  probes are the hot path of an incremental build, and equality on a fixed 32-byte
  array is a single `memcmp` with no allocation. The hex form is only materialized
  when a hash becomes a filename or a manifest key.
]

The same machinery hashes structured data, not just files: the `Config`, the build
context, the asset map, anything that implements `Hash` streams through a blake3
adapter. That comes back later.

== The hard part: dependencies through an opaque compiler

Content hashing tells you a page's _own_ source is unchanged. It says nothing about
the template it imports, or the module that template imports, or the JSON that
module loads. To rebuild correctly you need the transitive dependency set, and
Typst won't hand it to you.

But it will tell you if you watch. Baudelaire wraps the Typst `World`, the trait
the compiler calls whenever it wants a file, in a `Tracked` shim:

```rust
struct Tracked<W> {
    inner: W,
    accessed: Mutex<HashSet<FileId>>,
}
```

Every `source()` and every `file()` the compiler asks for gets recorded on the way
through. When compilation finishes, that set _is_ the dependency list: every import
it followed, every `json()` and `csv()` it loaded, every asset it read,
transitively, with zero static analysis. You don't parse the Typst to find the
imports; you let the compiler find them and write down what it touched.

Next build, a page is reused only if _every_ recorded dependency still hashes to
what it hashed before. Edit a transitive import three levels down and the pages
that touched it, and only those, fall out of the cache.

=== The comemo wrinkle

Typst's `World` is memoized by #link("https://github.com/typst/comemo")[comemo],
and it's shared across every page in the build. So there's a real worry: if page
B's compilation is served from a memoized result, the tracked accessors never run,
the `accessed` set comes back empty, and B looks like it has no dependencies. Get
that wrong and you serve stale HTML.

It holds up, because when comemo validates a memoized result it _re-invokes the
tracked accessors_ to check its assumptions still hold. The `source` and `file`
calls fire on the memoized path too, the set gets populated, and dependency
tracking stays correct even when the compile itself is skipped.

== Hash each file once, not once per dependent

A theme module imported by 300 pages would, done naively, get hashed 300 times per
build. So file digests go through a per-build memo:

```rust
Mutex<HashMap<PathBuf, Option<Hash>>>
```

First page to need a file hashes it; the rest read the cached digest. Misses are
memoized too: an unreadable or missing dependency is recorded as `None` so it isn't
re-`stat`-ed once per dependent.

#callout(kind: "note")[
  Dependencies are keyed by `PathBuf` serialized directly, not by its `Display`.
  A non-UTF-8 path would get lossily mangled by `Display` and then never match its
  real self next build, so the page rebuilds every time for no visible reason.
  Serializing the raw path round-trips it exactly.
]

== Two caches, because module evaluation is the real cost

There are two caches, because there are two expensive things and they fail
independently.

The *discovery cache* (`discovery.json`) stores each page's extracted frontmatter.
On an unchanged rebuild, evaluating the Typst module just to read its `frontmatter`
binding is the dominant cost, so a hit here skips module evaluation entirely. It
reads the file bytes, decodes them _exactly_ the way Typst builds a `Source` (strip
a leading BOM, nothing else), and if the hash matches, the module is never parsed.

The *compile cache* is a `manifest.json` plus a content-addressed `objects/` store.
The manifest holds metadata only, a `{ hash, deps, blob }` entry per page, so
loading it parses a small JSON instead of deserializing every page's rendered
markup. The HTML lives in `objects/ab/abcd…`, sharded by the first two hex
characters of _its own_ hash. Two pages that render to byte-identical HTML store
one blob and point at it twice. It's git's object store, scoped to one site.

A page is reused only if all of this holds:

- incremental builds are enabled,
- the manifest-level config fingerprint matches,
- the page's content hash equals the stored one,
- every dependency's current digest equals its stored digest,
- every build-metadata value the page read still hashes the same,
- the blob is still readable, and re-hashes to the stored address.

The torn-write clause guards a corner case: if the bytes on disk don't hash to the
address the manifest claims, it's a miss and the page rebuilds.

== When everything is stale at once

Some changes legitimately invalidate the whole site. A manifest-level fingerprint
folds together the `Config` and the render inputs: the processed-asset URL map, the
page-to-permalink map, and a hash of inlined assets when embedding is on. Change
your config or bump a fingerprinted asset and every page rebuilds, because those
inputs aren't visible to per-page dependency tracking.

== Build metadata, read by read

One input used to sit in that whole-site fingerprint and shouldn't have: build
metadata. A page reaches it through `sys.inputs.baudelaire`: `.git.hash`,
`.version`, `.date`. Folding it into the manifest meant every commit rebuilt every
page, including the ones that never mention it.

The fix is to treat a metadata read like any other dependency: find it, and rebuild
only when its value changes. The catch is that the read is a dictionary access
inside Typst, which the file tracker never sees. So Baudelaire reads it out of the
syntax tree instead. For each page it walks the closure of source it compiled and
resolves every expression rooted at `sys.inputs.baudelaire` down to the exact path
it reads: `git.hash`, not `git`, not the whole context. `let` aliases are
followed, so the idiomatic

```typ
#let build = sys.inputs.baudelaire
#let git = build.at("git", default: none)
... #git.hash ...
```

resolves to `git.hash` all the same. Each path is stored with the digest of its
value; a page is reused unless one of those digests moved. A new commit changes
`git.hash`, so the pages that print the commit rebuild and the rest stay cached. A
new day changes `date` and nothing else, so unless a page shows the date, nothing
rebuilds.

When an access can't be narrowed (a dynamic `.at(key)`, the whole object bound and
passed around), it widens to the entire context: correct, but coarse. Sound first,
precise where it can be.

== Writing it back

The save path runs under #link("https://github.com/rayon-rs/rayon")[rayon], in
parallel, so it has to handle races. Only new
blobs are written; an existing object path is left alone. Two pages staging
identical markup are de-duplicated so they don't fight over the same write. Blobs
are written once, atomically, via a temp sibling and a rename, so a crash mid-write
can't leave a half-blob. Then orphaned objects (a deleted page, a renamed
permalink, a dropped taxonomy term) are pruned, best-effort, since a housekeeping
failure shouldn't fail an otherwise-good build.

If the manifest itself is corrupt, the build warns and rebuilds from scratch rather
than treating garbage as an empty cache. When the cache can't be trusted, it's
thrown away.

The gap between an SSG that rebuilds in 40ms and one that rebuilds in 8 seconds is
entirely this: knowing, precisely, what you're allowed _not_ to do. The full
write-up lives in #link("../features/build/incremental.typ")[incremental builds].
