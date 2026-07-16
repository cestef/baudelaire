#let frontmatter = (
  title: "Bundling JavaScript in a static binary, no Node in sight",
  date: datetime(year: 2026, month: 7, day: 8),
  tags: ("blog", "internals", "rust"),
  summary: "A single Rust binary that still minifies CSS, bundles JavaScript, fingerprints filenames, and serves virtual modules, because rolldown and lightningcss are just crates, and a config value should exist exactly once whether Typst or the browser is reading it.",
)
#import "/templates/theme.typ": callout

Baudelaire is one binary. You download it, you run it, you get a website. No
runtime, no `node_modules`, no "first install these fourteen things." That
constraint is easy to hold right up until someone wants a real asset pipeline:
minified CSS, bundled and tree-shaken JavaScript, content-hashed filenames. And
the industry-standard answer is "put a Node toolchain next to your generator." Two
toolchains. Two install steps. Two things to break in CI.

In 2026 you don't have to. The interesting parts of the JavaScript toolchain have
been rewritten in Rust and shipped as crates, so they go _inside_ the binary.

== The bundler is a dependency, not a subprocess

JavaScript goes through #link("https://rolldown.rs")[rolldown], the oxc-based
bundler that powers Vite's next generation. Not shelled out to. Not a `pnpm`
script. A crate:

```toml
rolldown = "1"
```

The script handler owns a multi-threaded Tokio runtime, built once and reused for
every entry point, and drives rolldown's async `generate()` synchronously:

```rust
let output = self.runtime.block_on(bundler.generate())?;
```

ESM out, minification on when configured, and it pulls the chunk marked
`is_entry`. Files named `_thing.js` are import-only partials: they get bundled into
whatever imports them and emit nothing standalone, so you can split a module out
without it becoming its own page script.

CSS gets the same treatment through
#link("https://lightningcss.dev")[lightningcss]: parse, `minify`, print. PNGs go
through `oxipng`, JPEGs get re-encoded with their EXIF orientation baked in. Every
one of these is a Rust crate compiled into the binary. No Node anywhere, nothing to
install.

#callout(kind: "note")[
  Each is behind a Cargo feature (`js`, `css`, `images`). Build without one and
  files of that kind fall through to a verbatim byte-for-byte copy, so the pipeline
  degrades to "just copy it" instead of failing.
]

== Three phases, in order

Handlers run in three phases, because there's a dependency between them you can't
see until you trip on it.

- *Early.* Images and plain copies. They run first because fingerprinting renames
  them, and everything downstream needs the final names.
- *Late.* Stylesheets. A CSS file references images with `url()` and other sheets
  with `@import`, so it can only be finalized after those images have their hashed
  names.
- *Bundle.* Scripts, last. A bundle can import the map of every finalized asset, so
  it has to run once that map is complete.

Get the order wrong and a stylesheet points at `logo.png` while the file on disk is
`logo.a1b2c3d4.png`: a 404 that only shows up in production. The ordering is a type
(`Phase::Early | Late | Bundle`), not a property of iteration order.

Fingerprinting is 64 bits of blake3 spliced into the name (`app.css` becomes
`app.<16 hex chars>.css`), and the rewrite of every reference happens on Typst's
_typed_ HTML DOM, not by string replacement. It walks the tree and rewrites `href`,
`src`, `poster`, `content` (so `og:image` meta tags get caught), and every
candidate in a `srcset`. Working on parsed HTML means it can't mangle a string in a
code block that happens to look like a path.

== Virtual modules: `import` something that was never a file

Your client JavaScript can write:

```js
import { url } from "baudelaire:assets";
import config from "baudelaire:config";
```

There is no file called `baudelaire:assets`. It's a virtual module, served by a
single rolldown plugin that claims the specifier in `resolve_id` (so rolldown
doesn't go looking on disk) and answers with generated ES source in `load`. There
are eight of them: `search`, `site`, `config`, `assets`, `pages`, `sections`,
`taxonomies`, `feed`, each handing build-time data to the browser as a real,
tree-shakeable module. `baudelaire:assets` ships its own resolver:

```js
export function url(path) { return map[path] ?? path; }
```

The comment in the code calls `baudelaire:config` "baudelaire's answer to Vite's
`define`": build-time constants injected into the client, except it's a module you
`import` rather than a magic global the bundler string-replaces, so it tree-shakes
and it's typeable.

== One value, two languages

This data isn't assembled twice. A template reading `sys.inputs.baudelaire.site`
and a client script importing `baudelaire:site` read the _same value_, rendered to
two languages from one source of truth.

There's a single `codegen::Value` tree, and a `Format` trait with two
implementations, `TypstFmt` and `JsFmt`:

```rust
trait Format {
    fn string(&self, s: &str) -> String;
    fn dict(&self, entries: &[(String, Value)]) -> String;
    // ...
}
```

The same value tree feeds generated Typst source, generated JavaScript, and the
`sys.inputs` handed to the compiler. Typst escaping quotes `"` and `\` and knows an
empty dict is `(:)` and a one-element array needs a trailing comma; JS escaping
leans on `serde_json`, since a JSON string is a strict subset of a JS string.
Different targets, same data, so the build context your layout sees and the config
your browser sees can't drift apart.

For the client modules, `export default { ... }` is emitted _plus_ a named `export
const` for every top-level key that's a safe JS identifier, so `import { title }`
pulls one field and the bundler drops the rest.

== It's still one file

Client-side #link("../features/discovery/search.typ")[search] is one of those
virtual modules: turn it on and the build writes a static JSON index, the palette
UI imports `baudelaire:search`, rolldown bundles it, and it runs on that file with
no service and no third-party script. The
#link("../features/assets/assets.typ")[asset pipeline] minifies, bundles, and
fingerprints. rolldown and lightningcss did the hard work; the point is that
they're crates, so the thing you download is still one binary. No Node toolchain
beside the generator, no second install step. You run one file and you get a
website, asset pipeline and all.
